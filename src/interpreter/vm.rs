// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::blocks::Tipset;
use crate::chain::block_messages;
use crate::chain::index::ChainIndex;
use crate::chain::store::Error;
use crate::interpreter::{
    fvm2::ForestExternsV2, fvm3::ForestExterns as ForestExternsV3,
    fvm4::ForestExterns as ForestExternsV4,
};
use crate::message::ChainMessage;
use crate::message::Message as MessageTrait;
use crate::networks::{ChainConfig, NetworkChain};
use crate::shim::actors::{AwardBlockRewardParams, cron, reward};
use crate::shim::{
    address::Address,
    econ::TokenAmount,
    executor::{ApplyRet, Receipt, StampedEvent},
    externs::{Rand, RandWrapper},
    machine::MultiEngine,
    message::{Message, Message_v3},
    state_tree::ActorState,
    version::NetworkVersion,
};
use ahash::{HashMap, HashMapExt, HashSet};
use anyhow::bail;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{RawBytes, to_vec};
use fvm_shared2::clock::ChainEpoch;
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

pub(in crate::interpreter) type ForestMachineV2<DB> =
    DefaultMachine_v2<Arc<DB>, ForestExternsV2<DB>>;
pub(in crate::interpreter) type ForestMachineV3<DB> =
    DefaultMachine_v3<Arc<DB>, ForestExternsV3<DB>>;
pub(in crate::interpreter) type ForestMachineV4<DB> =
    DefaultMachine_v4<Arc<DB>, ForestExternsV4<DB>>;

type ForestKernelV2<DB> =
    fvm2::DefaultKernel<fvm2::call_manager::DefaultCallManager<ForestMachineV2<DB>>>;
type ForestKernelV3<DB> =
    fvm3::DefaultKernel<fvm3::call_manager::DefaultCallManager<ForestMachineV3<DB>>>;
type ForestKernelV4<DB> = fvm4::kernel::filecoin::DefaultFilecoinKernel<
    fvm4::call_manager::DefaultCallManager<ForestMachineV4<DB>>,
>;

type ForestExecutorV2<DB> = DefaultExecutor_v2<ForestKernelV2<DB>>;
type ForestExecutorV3<DB> = DefaultExecutor_v3<ForestKernelV3<DB>>;
type ForestExecutorV4<DB> = DefaultExecutor_v4<ForestKernelV4<DB>>;

pub type ApplyResult = anyhow::Result<(ApplyRet, Duration)>;

pub type ApplyBlockResult =
    anyhow::Result<(Vec<Receipt>, Vec<Vec<StampedEvent>>, Vec<Option<Cid>>), anyhow::Error>;

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

impl BlockMessages {
    /// Retrieves block messages to be passed through the VM and removes duplicate messages which appear in multiple blocks.
    pub fn for_tipset(db: &impl Blockstore, ts: &Tipset) -> Result<Vec<BlockMessages>, Error> {
        let mut applied = HashMap::new();
        let mut select_msg = |m: ChainMessage| -> Option<ChainMessage> {
            // The first match for a sender is guaranteed to have correct nonce
            // the block isn't valid otherwise.
            let entry = applied.entry(m.from()).or_insert_with(|| m.sequence());

            if *entry != m.sequence() {
                return None;
            }

            *entry += 1;
            Some(m)
        };

        ts.block_headers()
            .iter()
            .map(|b| {
                let (usm, sm) = block_messages(db, b)?;

                let mut messages = Vec::with_capacity(usm.len() + sm.len());
                messages.extend(
                    usm.into_iter()
                        .filter_map(|m| select_msg(ChainMessage::Unsigned(m))),
                );
                messages.extend(
                    sm.into_iter()
                        .filter_map(|m| select_msg(ChainMessage::Signed(m))),
                );

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
pub enum VM<DB: Blockstore + Send + Sync + 'static> {
    VM2(ForestExecutorV2<DB>),
    VM3(ForestExecutorV3<DB>),
    VM4(ForestExecutorV4<DB>),
}

pub struct ExecutionContext<DB> {
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
    pub chain_index: Arc<ChainIndex<Arc<DB>>>,
    // UNIX timestamp for epoch
    pub timestamp: u64,
}

impl<DB> VM<DB>
where
    DB: Blockstore + Send + Sync,
{
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
        }: ExecutionContext<DB>,
        multi_engine: &MultiEngine,
        enable_tracing: VMTrace,
    ) -> Result<Self, anyhow::Error> {
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

            let fvm: ForestMachineV4<DB> = ForestMachineV4::new(
                &context,
                Arc::clone(chain_index.db()),
                ForestExternsV4::new(
                    RandWrapper::from(rand),
                    heaviest_tipset,
                    epoch,
                    state_tree_root,
                    chain_index,
                    chain_config,
                ),
            )?;
            let exec: ForestExecutorV4<DB> = DefaultExecutor_v4::new(engine, fvm)?;
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

            let fvm: ForestMachineV3<DB> = ForestMachineV3::new(
                &context,
                Arc::clone(chain_index.db()),
                ForestExternsV3::new(
                    RandWrapper::from(rand),
                    heaviest_tipset,
                    epoch,
                    state_tree_root,
                    chain_index,
                    chain_config,
                ),
            )?;
            let exec: ForestExecutorV3<DB> = DefaultExecutor_v3::new(engine, fvm)?;
            Ok(VM::VM3(exec))
        } else {
            let config = NetworkConfig_v2::new(network_version.into());
            let engine = multi_engine.v2.get(&config)?;
            let mut context = config.for_epoch(epoch, state_tree_root);
            context.set_base_fee(base_fee.into());
            context.set_circulating_supply(circ_supply.into());
            context.tracing = enable_tracing.is_traced();

            let fvm: ForestMachineV2<DB> = ForestMachineV2::new(
                &engine,
                &context,
                Arc::clone(chain_index.db()),
                ForestExternsV2::new(
                    RandWrapper::from(rand),
                    heaviest_tipset,
                    epoch,
                    state_tree_root,
                    chain_index,
                    chain_config,
                ),
            )?;
            let exec: ForestExecutorV2<DB> = DefaultExecutor_v2::new(fvm);
            Ok(VM::VM2(exec))
        }
    }

    /// Flush stores in VM and return state root.
    pub fn flush(&mut self) -> anyhow::Result<Cid> {
        Ok(delegate_vm!(self.flush()?))
    }

    /// Get actor state from an address. Will be resolved to ID address.
    pub fn get_actor(&self, addr: &Address) -> Result<Option<ActorState>, anyhow::Error> {
        match self {
            VM::VM2(fvm_executor) => Ok(fvm_executor
                .state_tree()
                .get_actor(&addr.into())?
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
            anyhow::bail!("failed to apply block cron message: {}", err);
        }

        if let Some(mut callback) = callback {
            callback(MessageCallbackCtx {
                cid: cron_msg.cid(),
                message: &ChainMessage::Unsigned(cron_msg),
                apply_ret: &ret,
                at: CalledAt::Cron,
                duration,
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

            let mut process_msg = |message: &ChainMessage| -> Result<(), anyhow::Error> {
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
                    })?;
                }

                // Update totals
                gas_reward += ret.miner_tip();
                penalty += ret.penalty();
                let msg_receipt = ret.msg_receipt();
                receipts.push(msg_receipt.clone());

                events_roots.push(ret.msg_receipt().events_root());
                events.push(ret.events());

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
                if !ret.msg_receipt().exit_code().is_success() {
                    anyhow::bail!(
                        "reward application message failed (exit: {:?})",
                        ret.msg_receipt().exit_code()
                    );
                }

                if let Some(callback) = &mut callback {
                    callback(MessageCallbackCtx {
                        cid: rew_msg.cid(),
                        message: &ChainMessage::Unsigned(rew_msg),
                        apply_ret: &ret,
                        at: CalledAt::Reward,
                        duration,
                    })?
                }
            }
        }

        if let Err(e) = self.run_cron(epoch, callback.as_mut()) {
            tracing::error!("End of epoch cron failed to run: {}", e);
        }

        Ok((receipts, events, events_roots))
    }

    /// Applies single message through VM and returns result from execution.
    pub fn apply_implicit_message(&mut self, msg: &Message) -> ApplyResult {
        let start = Instant::now();

        // raw_length is not used for Implicit messages.
        let raw_length = to_vec(msg).expect("encoding error").len();

        let ret = match self {
            VM::VM2(fvm_executor) => fvm_executor
                .execute_message(msg.into(), fvm2::executor::ApplyKind::Implicit, raw_length)?
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
        let raw_length = to_vec(msg).expect("encoding error").len();
        let ret: ApplyRet = match self {
            VM::VM2(fvm_executor) => {
                let ret = fvm_executor.execute_message(
                    unsigned.into(),
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

        let exit_code = ret.msg_receipt().exit_code();

        if !exit_code.is_success() {
            tracing::debug!(?exit_code, "VM message execution failure.")
        }

        Ok((ret, duration))
    }

    fn reward_message(
        &self,
        epoch: ChainEpoch,
        miner: Address,
        win_count: i64,
        penalty: TokenAmount,
        gas_reward: TokenAmount,
    ) -> Result<Option<Message>, anyhow::Error> {
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
}

#[derive(Debug, Clone, Copy)]
pub enum CalledAt {
    Applied,
    Reward,
    Cron,
}

impl CalledAt {
    /// Was [`VM::apply_message`] or [`VM::apply_implicit_message`] called?
    pub fn apply_kind(&self) -> fvm3::executor::ApplyKind {
        use fvm3::executor::ApplyKind;
        match self {
            CalledAt::Applied => ApplyKind::Explicit,
            CalledAt::Reward | CalledAt::Cron => ApplyKind::Implicit,
        }
    }
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
