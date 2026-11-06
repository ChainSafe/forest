// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Tipset;
use crate::chain::block_messages;
use crate::chain::index::ChainIndex;
use crate::chain::store::Error;
use crate::db::DbImpl;
use crate::interpreter::{fvm2::ForestExternsV2, fvm3::ForestExternsV3, fvm4::ForestExternsV4};
use crate::message::ChainMessage;
use crate::message::MessageRead as _;
use crate::networks::{ChainConfig, Height, NetworkChain};
use crate::prelude::*;
use crate::shim::actors::{AwardBlockRewardParams, cron, reward};
use crate::shim::clock::ChainEpoch;
use crate::shim::{
    address::{Address, Protocol},
    econ::TokenAmount,
    executor::{ApplyRet, Receipt, StampedEvent},
    externs::Rand,
    machine::MultiEngine,
    message::{Message, Message_v3},
    state_tree::{ActorState, StateTree},
    version::NetworkVersion,
};
use ahash::{HashMap, HashSet};
use anyhow::bail;
use fvm_ipld_encoding::RawBytes;
use fvm2::{
    executor::{DefaultExecutor as DefaultExecutor_v2, Executor as Executor_v2},
    machine::{
        DefaultMachine as DefaultMachine_v2, Machine as Machine_v2,
        NetworkConfig as NetworkConfig_v2,
    },
};
use fvm3::{
    executor::{DefaultExecutor as DefaultExecutor_v3, Executor as Executor_v3},
    machine::{
        DefaultMachine as DefaultMachine_v3, Machine as Machine_v3,
        NetworkConfig as NetworkConfig_v3,
    },
};
use fvm4::{
    executor::{DefaultExecutor as DefaultExecutor_v4, Executor as Executor_v4},
    machine::{
        DefaultMachine as DefaultMachine_v4, Machine as Machine_v4,
        NetworkConfig as NetworkConfig_v4,
    },
};
use num::Zero;
use spire_enum::prelude::delegated_enum;
use std::time::{Duration, Instant};

pub(in crate::interpreter) type ForestMachineV2 = DefaultMachine_v2<DbImpl, ForestExternsV2>;
pub(in crate::interpreter) type ForestMachineV3 = DefaultMachine_v3<DbImpl, ForestExternsV3>;
pub(in crate::interpreter) type ForestMachineV4 = DefaultMachine_v4<DbImpl, ForestExternsV4>;

type ForestKernelV2 = fvm2::DefaultKernel<fvm2::call_manager::DefaultCallManager<ForestMachineV2>>;
type ForestKernelV3 = fvm3::DefaultKernel<fvm3::call_manager::DefaultCallManager<ForestMachineV3>>;
type ForestKernelV4 = fvm4::kernel::filecoin::DefaultFilecoinKernel<
    fvm4::call_manager::DefaultCallManager<ForestMachineV4>,
>;

type ForestExecutorV2 = DefaultExecutor_v2<ForestKernelV2>;
type ForestExecutorV3 = DefaultExecutor_v3<ForestKernelV3>;
type ForestExecutorV4 = DefaultExecutor_v4<ForestKernelV4>;

pub type ApplyResult = anyhow::Result<(ApplyRet, Duration)>;

pub type ApplyBlockResult = anyhow::Result<(
    Vec<Receipt>,
    Vec<Option<Vec<StampedEvent>>>,
    Vec<Option<Cid>>,
)>;

/// Comes from <https://github.com/filecoin-project/lotus/blob/v1.23.2/chain/vm/fvm.go#L473>
pub const IMPLICIT_MESSAGE_GAS_LIMIT: i64 = i64::MAX / 2;

/// Contains all messages to process through the VM as well as miner information
/// for block rewards.
#[derive(Debug)]
pub struct BlockMessages {
    pub miner: Address,
    pub messages: Vec<ChainMessage>,
    pub win_count: i64,
}

/// How tipset message senders are keyed for dedup, mirroring Lotus's lazy ID resolution.
enum Keying<'a, DB> {
    /// Pre-Hyperdrive: key by the raw sender address, never resolving.
    Raw,
    /// Hyperdrive era with no ID-form sender yet: key by the raw address, but resolve every sender
    /// to its ID address as soon as one appears.
    Lazy { db: &'a DB, parent_state: &'a Cid },
    /// An ID-form sender has appeared: key by the resolved ID address.
    Resolved(Box<StateTree<DB>>),
}

impl<DB: Blockstore + ShallowClone> Keying<'_, DB> {
    /// The dedup key for `from`. On the first ID-form sender, loads the parent state, re-keys the
    /// `applied` entries so far to their ID addresses, and switches to [`Keying::Resolved`].
    fn sender_key(
        &mut self,
        from: Address,
        applied: &mut HashMap<Address, u64>,
    ) -> Result<Address, Error> {
        match self {
            Keying::Raw => Ok(from),
            Keying::Resolved(state) => Ok(state.lookup_required_id_address(&from)?),
            Keying::Lazy { db, parent_state } if from.protocol() == Protocol::ID => {
                let (db, parent_state) = (*db, *parent_state);
                let state = StateTree::new_from_root(db, parent_state)?;
                for (robust, nonce) in std::mem::take(applied) {
                    applied.insert(state.lookup_required_id_address(&robust)?, nonce);
                }
                let id = state.lookup_required_id_address(&from)?;
                *self = Keying::Resolved(Box::new(state));
                Ok(id)
            }
            Keying::Lazy { .. } => Ok(from),
        }
    }
}

impl BlockMessages {
    /// Retrieves block messages to be passed through the VM and removes duplicate messages which appear in multiple blocks.
    pub fn for_tipset<DB: Blockstore + ShallowClone>(
        db: &DB,
        ts: &Tipset,
        chain_config: &ChainConfig,
    ) -> Result<Vec<BlockMessages>, Error> {
        // From Hyperdrive, dedup keys on the sender's resolved ID, lazily as in Lotus `BlockMsgsForTipset`.
        // https://github.com/filecoin-project/lotus/blob/0065aa70bb20bf87801b6e96ecb7e7fe707566bd/chain/store/messages.go#L126-L182
        let mut applied: HashMap<Address, u64> = HashMap::new();
        let mut keying = if ts.epoch() >= chain_config.epoch(Height::Hyperdrive) {
            Keying::Lazy {
                db,
                parent_state: ts.parent_state(),
            }
        } else {
            Keying::Raw
        };
        let mut select_msg = |m: ChainMessage| -> Result<Option<ChainMessage>, Error> {
            let sender = keying.sender_key(m.from(), &mut applied)?;
            // The first match for a sender is guaranteed to have correct nonce
            // the block isn't valid otherwise.
            let entry = applied.entry(sender).or_insert_with(|| m.sequence());

            if *entry != m.sequence() {
                return Ok(None);
            }

            *entry += 1;
            Ok(Some(m))
        };

        ts.block_headers()
            .iter()
            .map(|b| {
                let (usm, sm) = block_messages(db, b)?;

                let mut messages = Vec::with_capacity(usm.len() + sm.len());
                for m in usm {
                    messages.extend(select_msg(m.into())?);
                }
                for m in sm {
                    messages.extend(select_msg(m.into())?);
                }

                Ok(BlockMessages {
                    miner: b.miner_address,
                    messages,
                    win_count: b
                        .election_proof
                        .as_ref()
                        .map(|e| e.win_count)
                        .unwrap_or_default(),
                })
            })
            .collect()
    }
}

/// Interpreter which handles execution of state transitioning messages and
/// returns receipts from the VM execution.
#[delegated_enum(impl_conversions)]
pub enum VM {
    VM2(ForestExecutorV2),
    VM3(ForestExecutorV3),
    VM4(ForestExecutorV4),
}

pub struct ExecutionContext {
    // This tipset identifies of the blockchain. It functions as a starting
    // point when searching for ancestors. It may be any tipset as long as its
    // epoch is at or higher than the epoch in `epoch`.
    pub heaviest_tipset: Tipset,
    // State-tree generated by the parent tipset.
    pub state_tree_root: Cid,
    // Epoch of the messages to be executed.
    pub epoch: ChainEpoch,
    // Source of deterministic randomness
    pub rand: Box<dyn Rand>,
    // https://spec.filecoin.io/systems/filecoin_vm/gas_fee/
    pub base_fee: TokenAmount,
    // https://filecoin.io/blog/filecoin-circulating-supply/
    pub circ_supply: TokenAmount,
    // The chain config is used to determine which consensus rules to use.
    pub chain_config: Arc<ChainConfig>,
    // Caching interface to the DB
    pub chain_index: ChainIndex,
    // UNIX timestamp for epoch
    pub timestamp: u64,
}

impl VM {
    pub fn new(
        ExecutionContext {
            heaviest_tipset,
            state_tree_root,
            epoch,
            rand,
            base_fee,
            circ_supply,
            chain_config,
            chain_index,
            timestamp,
        }: ExecutionContext,
        multi_engine: &MultiEngine,
        enable_tracing: VMTrace,
    ) -> anyhow::Result<Self> {
        let network_version = chain_config.network_version(epoch);
        if network_version >= NetworkVersion::V21 {
            let mut config = NetworkConfig_v4::new(network_version.into());
            // ChainId defines the chain ID used in the Ethereum JSON-RPC endpoint.
            config.chain_id((chain_config.eth_chain_id).into());
            if let NetworkChain::Devnet(_) = chain_config.network {
                config.enable_actor_debugging();
            }

            let engine = multi_engine.v4.get(&config)?;
            let mut context = config.for_epoch(epoch, timestamp, state_tree_root);
            context.set_base_fee(base_fee.into());
            context.set_circulating_supply(circ_supply.into());
            context.tracing = enable_tracing.is_traced();

            let fvm = ForestMachineV4::new(
                &context,
                chain_index.db().shallow_clone(),
                ForestExternsV4::new(
                    rand,
                    heaviest_tipset,
                    epoch,
                    state_tree_root,
                    chain_index,
                    chain_config,
                ),
            )?;
            let exec = DefaultExecutor_v4::new(engine, fvm)?;
            Ok(VM::VM4(exec))
        } else if network_version >= NetworkVersion::V18 {
            let mut config = NetworkConfig_v3::new(network_version.into());
            // ChainId defines the chain ID used in the Ethereum JSON-RPC endpoint.
            config.chain_id((chain_config.eth_chain_id).into());
            if let NetworkChain::Devnet(_) = chain_config.network {
                config.enable_actor_debugging();
            }

            let engine = multi_engine.v3.get(&config)?;
            let mut context = config.for_epoch(epoch, timestamp, state_tree_root);
            context.set_base_fee(base_fee.into());
            context.set_circulating_supply(circ_supply.into());
            context.tracing = enable_tracing.is_traced();

            let fvm = ForestMachineV3::new(
                &context,
                chain_index.db().shallow_clone(),
                ForestExternsV3::new(
                    rand,
                    heaviest_tipset,
                    epoch,
                    state_tree_root,
                    chain_index,
                    chain_config,
                ),
            )?;
            let exec = DefaultExecutor_v3::new(engine, fvm)?;
            Ok(VM::VM3(exec))
        } else {
            let config = NetworkConfig_v2::new(network_version.into());
            let engine = multi_engine.v2.get(&config)?;
            let mut context = config.for_epoch(epoch, state_tree_root);
            context.set_base_fee(base_fee.into());
            context.set_circulating_supply(circ_supply.into());
            context.tracing = enable_tracing.is_traced();

            let fvm = ForestMachineV2::new(
                &engine,
                &context,
                chain_index.db().shallow_clone(),
                ForestExternsV2::new(
                    rand,
                    heaviest_tipset,
                    epoch,
                    state_tree_root,
                    chain_index,
                    chain_config,
                ),
            )?;
            let exec = DefaultExecutor_v2::new(fvm);
            Ok(VM::VM2(exec))
        }
    }

    /// Flush stores in VM and return state root.
    pub fn flush(&mut self) -> anyhow::Result<Cid> {
        Ok(delegate_vm!(self.flush()?))
    }

    /// Get actor state from an address. Will be resolved to ID address.
    pub fn get_actor(&self, addr: &Address) -> anyhow::Result<Option<ActorState>> {
        match self {
            VM::VM2(fvm_executor) => Ok(fvm_executor
                .state_tree()
                .get_actor(&addr.try_to_v2()?)?
                .map(ActorState::from)),
            VM::VM3(fvm_executor) => {
                if let Some(id) = fvm_executor.state_tree().lookup_id(&addr.into())? {
                    Ok(fvm_executor
                        .state_tree()
                        .get_actor(id)?
                        .map(ActorState::from))
                } else {
                    Ok(None)
                }
            }
            VM::VM4(fvm_executor) => {
                if let Some(id) = fvm_executor.state_tree().lookup_id(&addr.into())? {
                    Ok(fvm_executor
                        .state_tree()
                        .get_actor(id)?
                        .map(ActorState::from))
                } else {
                    Ok(None)
                }
            }
        }
    }

    pub fn run_cron(
        &mut self,
        epoch: ChainEpoch,
        callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()>>,
    ) -> anyhow::Result<()> {
        let cron_msg: Message = Message_v3 {
            from: Address::SYSTEM_ACTOR.into(),
            to: Address::CRON_ACTOR.into(),
            // Epoch as sequence is intentional
            sequence: epoch as u64,
            // Arbitrarily large gas limit for cron (matching Lotus value)
            gas_limit: IMPLICIT_MESSAGE_GAS_LIMIT as u64,
            method_num: cron::Method::EpochTick as u64,
            params: Default::default(),
            value: Default::default(),
            version: Default::default(),
            gas_fee_cap: Default::default(),
            gas_premium: Default::default(),
        }
        .into();

        let (ret, duration) = self.apply_implicit_message(&cron_msg)?;
        if let Some(err) = ret.failure_info() {
            anyhow::bail!("failed to apply block cron message: {err}");
        }

        if let Some(mut callback) = callback {
            callback(MessageCallbackCtx {
                cid: cron_msg.cid(),
                message: &cron_msg.into(),
                apply_ret: &ret,
                at: CalledAt::Cron,
                duration,
                state: VMStateView(self),
            })?;
        }
        Ok(())
    }

    /// Apply block messages from a Tipset.
    /// Returns the receipts from the transactions.
    pub fn apply_block_messages(
        &mut self,
        messages: &[BlockMessages],
        epoch: ChainEpoch,
        mut callback: Option<impl FnMut(MessageCallbackCtx<'_>) -> anyhow::Result<()>>,
    ) -> ApplyBlockResult {
        let mut receipts = Vec::new();
        let mut events = Vec::new();
        let mut events_roots: Vec<Option<Cid>> = Vec::new();
        let mut processed = HashSet::default();

        for block in messages.iter() {
            let mut penalty = TokenAmount::zero();
            let mut gas_reward = TokenAmount::zero();

            let mut process_msg = |message: &ChainMessage| -> anyhow::Result<()> {
                let cid = message.cid();
                // Ensure no duplicate processing of a message
                if processed.contains(&cid) {
                    return Ok(());
                }
                let (ret, duration) = self.apply_message(message)?;

                if let Some(cb) = &mut callback {
                    cb(MessageCallbackCtx {
                        cid,
                        message,
                        apply_ret: &ret,
                        at: CalledAt::Applied,
                        duration,
                        state: VMStateView(self),
                    })?;
                }

                // Update totals
                gas_reward += ret.miner_tip();
                penalty += ret.penalty();
                let (receipt, msg_events) = ret.into_receipt_and_events();
                events_roots.push(receipt.events_root());
                receipts.push(receipt);
                events.push(msg_events);

                // Add processed Cid to set of processed messages
                processed.insert(cid);
                Ok(())
            };

            for msg in block.messages.iter() {
                process_msg(msg)?;
            }

            // Generate reward transaction for the miner of the block
            if let Some(rew_msg) =
                self.reward_message(epoch, block.miner, block.win_count, penalty, gas_reward)?
            {
                let (ret, duration) = self.apply_implicit_message(&rew_msg)?;
                if let Some(err) = ret.failure_info() {
                    anyhow::bail!(
                        "failed to apply reward message for miner {}: {}",
                        block.miner,
                        err
                    );
                }
                // This is more of a sanity check, this should not be able to be hit.
                if !ret.exit_code().is_success() {
                    anyhow::bail!(
                        "reward application message failed (exit: {:?})",
                        ret.exit_code()
                    );
                }

                if let Some(callback) = &mut callback {
                    callback(MessageCallbackCtx {
                        cid: rew_msg.cid(),
                        message: &rew_msg.into(),
                        apply_ret: &ret,
                        at: CalledAt::Reward,
                        duration,
                        state: VMStateView(self),
                    })?
                }
            }
        }

        if let Err(e) = self.run_cron(epoch, callback.as_mut()) {
            tracing::error!("End of epoch cron failed to run: {e:#}");
        }

        Ok((receipts, events, events_roots))
    }

    /// Applies single message through VM and returns result from execution.
    pub fn apply_implicit_message(&mut self, msg: &Message) -> ApplyResult {
        let start = Instant::now();

        // Every FVM version charges a zero `GasCharge` for `ApplyKind::Implicit` and never reads
        // `raw_length`, so don't pay to serialize the message just to measure it.
        let raw_length = 0;

        let ret = match self {
            VM::VM2(fvm_executor) => fvm_executor
                .execute_message(
                    msg.try_into()?,
                    fvm2::executor::ApplyKind::Implicit,
                    raw_length,
                )?
                .into(),
            VM::VM3(fvm_executor) => fvm_executor
                .execute_message(msg.into(), fvm3::executor::ApplyKind::Implicit, raw_length)?
                .into(),
            VM::VM4(fvm_executor) => fvm_executor
                .execute_message(msg.into(), fvm4::executor::ApplyKind::Implicit, raw_length)?
                .into(),
        };
        Ok((ret, start.elapsed()))
    }

    /// Applies the state transition for a single message.
    /// Returns `ApplyRet` structure which contains the message receipt and some
    /// meta data.
    pub fn apply_message(&mut self, msg: &ChainMessage) -> ApplyResult {
        let start = Instant::now();

        // Basic validity check
        msg.message().check()?;

        let unsigned = msg.message().clone();
        let raw_length = msg.encoded_len();
        let ret: ApplyRet = match self {
            VM::VM2(fvm_executor) => {
                let ret = fvm_executor.execute_message(
                    unsigned.try_into()?,
                    fvm2::executor::ApplyKind::Explicit,
                    raw_length,
                )?;

                if fvm_executor.externs().bail() {
                    bail!("encountered a database lookup error");
                }

                ret.into()
            }
            VM::VM3(fvm_executor) => {
                let ret = fvm_executor.execute_message(
                    unsigned.into(),
                    fvm3::executor::ApplyKind::Explicit,
                    raw_length,
                )?;

                if fvm_executor.externs().bail() {
                    bail!("encountered a database lookup error");
                }

                ret.into()
            }
            VM::VM4(fvm_executor) => {
                let ret = fvm_executor.execute_message(
                    unsigned.into(),
                    fvm4::executor::ApplyKind::Explicit,
                    raw_length,
                )?;

                if fvm_executor.externs().bail() {
                    bail!("encountered a database lookup error");
                }

                ret.into()
            }
        };
        let duration = start.elapsed();

        let exit_code = ret.exit_code();

        if !exit_code.is_success() {
            tracing::debug!(?exit_code, "VM message execution failure.")
        }

        Ok((ret, duration))
    }

    pub(crate) fn reward_message(
        &self,
        epoch: ChainEpoch,
        miner: Address,
        win_count: i64,
        penalty: TokenAmount,
        gas_reward: TokenAmount,
    ) -> anyhow::Result<Option<Message>> {
        let params = RawBytes::serialize(AwardBlockRewardParams {
            miner: miner.into(),
            penalty: penalty.into(),
            gas_reward: gas_reward.into(),
            win_count,
        })?;
        let rew_msg = Message_v3 {
            from: Address::SYSTEM_ACTOR.into(),
            to: Address::REWARD_ACTOR.into(),
            method_num: reward::Method::AwardBlockReward as u64,
            params,
            // Epoch as sequence is intentional
            sequence: epoch as u64,
            gas_limit: IMPLICIT_MESSAGE_GAS_LIMIT as u64,
            value: Default::default(),
            version: Default::default(),
            gas_fee_cap: Default::default(),
            gas_premium: Default::default(),
        };
        Ok(Some(rew_msg.into()))
    }
}

#[derive(Debug, Clone)]
pub struct MessageCallbackCtx<'a> {
    pub cid: Cid,
    pub message: &'a ChainMessage,
    pub apply_ret: &'a ApplyRet,
    pub at: CalledAt,
    pub duration: Duration,
    /// State of the VM after `message` was applied.
    pub state: VMStateView<'a>,
}

/// Read-only view of a [`VM`]'s state, including the changes it has not flushed.
#[derive(Clone, Copy, derive_more::Debug)]
pub struct VMStateView<'a>(#[debug(skip)] &'a VM);

impl VMStateView<'_> {
    /// Returns the state of the actor at `addr`, or `None` when there is no such actor.
    pub fn get_actor(&self, addr: &Address) -> anyhow::Result<Option<ActorState>> {
        self.0.get_actor(addr)
    }
}

impl Blockstore for VMStateView<'_> {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        delegate_vm!(self.0 => |executor| executor.state_tree().store().get(k))
    }

    /// Not supported, the view is read-only.
    fn put_keyed(&self, _: &Cid, _: &[u8]) -> anyhow::Result<()> {
        bail!("VMStateView is read-only");
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CalledAt {
    Applied,
    Reward,
    Cron,
}

/// Tracing a Filecoin VM has a performance penalty.
/// This controls whether a VM should be traced or not when it is created.
#[derive(Default, Clone, Copy)]
pub enum VMTrace {
    /// Collect trace for the given operation
    Traced,
    /// Do not collect trace
    #[default]
    NotTraced,
}

impl VMTrace {
    /// Should tracing be collected?
    pub fn is_traced(&self) -> bool {
        matches!(self, VMTrace::Traced)
    }
}

#[cfg(test)]
mod for_tipset_tests {
    use super::*;
    use crate::blocks::{Block, CachingBlockHeader, FullTipset, RawBlockHeader, Tipset};
    use crate::chain_sync::TipsetValidator;
    use crate::db::{DbImpl, MemoryDB};
    use crate::message::SignedMessage;
    use crate::shim::crypto::Signature;
    use crate::shim::message::Message_v3;
    use crate::test_utils::{dummy_ticket, resolving_state_tree};
    use rstest::rstest;

    fn secp(from: Address, nonce: u64) -> SignedMessage {
        let m: Message = Message_v3 {
            to: Address::new_id(99).into(),
            from: from.into(),
            sequence: nonce,
            ..Default::default()
        }
        .into();
        SignedMessage::new_unchecked(m, Signature::new_secp256k1(vec![0]))
    }

    fn selected_message_count(
        db: &DbImpl,
        secp_messages: Vec<SignedMessage>,
        state_root: Cid,
        epoch: ChainEpoch,
    ) -> usize {
        let config = ChainConfig::default();
        let secp_cids = secp_messages.iter().map(SignedMessage::signed_cid);
        let msg_root =
            TipsetValidator::compute_msg_root(&db, std::iter::empty(), secp_cids).unwrap();
        let header = CachingBlockHeader::new(RawBlockHeader {
            miner_address: Address::new_id(1000),
            ticket: dummy_ticket(0),
            epoch,
            state_root,
            messages: msg_root,
            ..Default::default()
        });
        let block = Block {
            header,
            bls_messages: vec![],
            secp_messages: secp_messages.into_iter().map(Arc::new).collect(),
        };
        let fts = FullTipset::new([block]).unwrap();
        fts.persist(&db).unwrap();
        let tipset = Tipset::from(fts);

        BlockMessages::for_tipset(db, &tipset, &config)
            .unwrap()
            .iter()
            .map(|bm| bm.messages.len())
            .sum()
    }

    #[rstest]
    #[case::from_hyperdrive_dedups(0, 1)]
    #[case::before_hyperdrive_keeps_both(-1, 2)]
    fn one_account_under_two_address_forms(
        #[case] epoch_offset: ChainEpoch,
        #[case] expected: usize,
    ) {
        let db: DbImpl = Arc::new(MemoryDB::default()).into();
        let f1 = Address::new_secp256k1(&[0x04; 65]).unwrap();
        let (mut tree, f0) = resolving_state_tree(&db, f1);
        let state_root = tree.flush().unwrap();

        let count = selected_message_count(
            &db,
            vec![secp(f1, 7), secp(f0, 7)],
            state_root,
            ChainConfig::default().epoch(Height::Hyperdrive) + epoch_offset,
        );
        assert_eq!(count, expected);
    }

    #[test]
    fn re_key_preserves_nonces() {
        let db: DbImpl = Arc::new(MemoryDB::default()).into();
        let f1 = Address::new_secp256k1(&[0x04; 65]).unwrap();
        let (mut tree, f0) = resolving_state_tree(&db, f1);
        let state_root = tree.flush().unwrap();

        // Re-keying to the ID must carry the nonce advanced under `f1`, leaving only `f0` as the
        // duplicate.
        let count = selected_message_count(
            &db,
            vec![secp(f1, 7), secp(f1, 8), secp(f0, 8)],
            state_root,
            ChainConfig::default().epoch(Height::Hyperdrive),
        );
        assert_eq!(count, 2);
    }

    #[test]
    fn robust_only_senders_need_no_state() {
        let db: DbImpl = Arc::new(MemoryDB::default()).into();
        let f1 = Address::new_secp256k1(&[0x04; 65]).unwrap();

        // The state root is absent from the store; lazy keying must not load it while every sender
        // is in robust form, as happens for a pruned tipset.
        let count = selected_message_count(
            &db,
            vec![secp(f1, 7), secp(f1, 8)],
            Cid::default(),
            ChainConfig::default().epoch(Height::Hyperdrive),
        );
        assert_eq!(count, 2);
    }
}
