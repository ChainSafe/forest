// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Rand;
use actor::{
    actorv0::reward::AwardBlockRewardParams, cron, miner, reward, system, BURNT_FUNDS_ACTOR_ADDR,
};
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use fil_types::BLOCK_GAS_LIMIT;
use fil_types::{
    verifier::{FullVerifier, ProofVerifier},
    DefaultNetworkParams, NetworkParams,
};
use forest_encoding::Cbor;
use fvm::machine::Engine;
use ipld_blockstore::BlockStore;
use ipld_blockstore::FvmStore;
use message::{ChainMessage, Message, MessageReceipt, UnsignedMessage};
use networks::{UPGRADE_ACTORS_V4_HEIGHT, UPGRADE_CLAUS_HEIGHT};
use num_bigint::BigInt;
use state_tree::StateTree;
use std::collections::HashSet;
use std::error::Error as StdError;
use std::marker::PhantomData;
use std::sync::Arc;
use vm::{ActorError, ExitCode, Serialized, TokenAmount};

const GAS_OVERUSE_NUM: i64 = 11;
const GAS_OVERUSE_DENOM: i64 = 10;

/// Contains all messages to process through the VM as well as miner information for block rewards.
#[derive(Debug)]
pub struct BlockMessages {
    pub miner: Address,
    pub messages: Vec<ChainMessage>,
    pub win_count: i64,
}

/// Allows generation of the current circulating supply
/// given some context.
pub trait CircSupplyCalc {
    /// Retrieves total circulating supply on the network.
    fn get_supply<DB: BlockStore>(
        &self,
        height: ChainEpoch,
        state_tree: &StateTree<DB>,
    ) -> Result<TokenAmount, Box<dyn StdError>>;
}

/// Trait to allow VM to retrieve state at an old epoch.
pub trait LookbackStateGetter<'db, DB> {
    /// Returns a state tree from the given epoch.
    fn state_lookback(&self, epoch: ChainEpoch) -> Result<StateTree<'db, DB>, Box<dyn StdError>>;
}

use crypto::DomainSeparationTag;
use fvm::externs::Consensus;
use fvm::externs::Externs;
use fvm_shared::consensus::ConsensusFault;

pub struct ForestExterns {
    rand: Box<dyn Rand>,
}

impl ForestExterns {
    fn new(rand: impl Rand + 'static) -> Self {
        ForestExterns {
            rand: Box::new(rand),
        }
    }
}

impl Externs for ForestExterns {}

impl Rand for ForestExterns {
    fn get_chain_randomness(
        &self,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        self.rand.get_chain_randomness(pers, round, entropy)
    }

    fn get_beacon_randomness(
        &self,
        pers: DomainSeparationTag,
        round: ChainEpoch,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        self.rand.get_beacon_randomness(pers, round, entropy)
    }
}

impl Consensus for ForestExterns {
    fn verify_consensus_fault(
        &self,
        _h1: &[u8],
        _h2: &[u8],
        _extra: &[u8],
    ) -> anyhow::Result<Option<ConsensusFault>> {
        todo!()
    }
}

use fvm::machine::{Machine, MachineContext};
use fvm::state_tree::ActorState;
use fvm::Config;

pub struct ForestMachine<DB: 'static> {
    machine: fvm::machine::DefaultMachine<FvmStore<DB>, ForestExterns>,
}

impl<DB: BlockStore> Machine for ForestMachine<DB> {
    type Blockstore =
        <fvm::machine::DefaultMachine<FvmStore<DB>, ForestExterns> as Machine>::Blockstore;
    type Externs = ForestExterns;

    fn engine(&self) -> &wasmtime::Engine {
        self.machine.engine()
    }

    fn config(&self) -> &Config {
        self.machine.config()
    }

    fn blockstore(&self) -> &Self::Blockstore {
        self.machine.blockstore()
    }

    fn context(&self) -> &MachineContext {
        self.machine.context()
    }

    fn externs(&self) -> &Self::Externs {
        self.machine.externs()
    }

    fn state_tree(&self) -> &fvm::state_tree::StateTree<Self::Blockstore> {
        self.machine.state_tree()
    }

    fn state_tree_mut(&mut self) -> &mut fvm::state_tree::StateTree<Self::Blockstore> {
        self.machine.state_tree_mut()
    }

    fn create_actor(
        &mut self,
        addr: &fvm_shared::address::Address,
        act: ActorState,
    ) -> fvm::kernel::Result<ActorID> {
        self.machine.create_actor(addr, act)
    }

    fn load_module(&self, code: &cid_orig::Cid) -> fvm::kernel::Result<wasmtime::Module> {
        self.machine.load_module(code)
    }

    fn transfer(
        &mut self,
        from: ActorID,
        to: ActorID,
        value: &TokenAmount,
    ) -> fvm::kernel::Result<()> {
        self.machine.transfer(from, to, value)
    }

    fn consume(self) -> Self::Blockstore {
        self.machine.consume()
    }

    fn flush(&mut self) -> fvm::kernel::Result<cid_orig::Cid> {
        self.machine.flush()
    }
}

pub struct ForestKernel<DB: BlockStore + 'static>(
    fvm::DefaultKernel<fvm::call_manager::DefaultCallManager<ForestMachine<DB>>>,
);

use crate::{price_list_by_epoch, DefaultRuntime, GasCharge};
use fvm::call_manager::*;
use fvm::kernel::{BlockId, BlockStat};
use fvm_shared::bigint::Sign;
use fvm_shared::crypto::signature::Signature;
use fvm_shared::piece::PieceInfo;
use fvm_shared::randomness::RANDOMNESS_LENGTH;
use fvm_shared::sector::*;
use fvm_shared::version::NetworkVersion;
use fvm_shared::{ActorID, MethodNum};
use log::debug;
use num_traits::Zero;

impl<DB: BlockStore> fvm::Kernel for ForestKernel<DB> {
    type CallManager = fvm::call_manager::DefaultCallManager<ForestMachine<DB>>;

    fn take(self) -> Self::CallManager
    where
        Self: Sized,
    {
        self.0.take()
    }

    fn new(
        mgr: Self::CallManager,
        from: fvm_shared::ActorID,
        to: fvm_shared::ActorID,
        method: fvm_shared::MethodNum,
        value_received: TokenAmount,
    ) -> Self
    where
        Self: Sized,
    {
        ForestKernel(fvm::DefaultKernel::new(
            mgr,
            from,
            to,
            method,
            value_received,
        ))
    }
}
impl<DB: BlockStore> fvm::kernel::ActorOps for ForestKernel<DB> {
    fn resolve_address(
        &self,
        address: &fvm_shared::address::Address,
    ) -> fvm::kernel::Result<Option<ActorID>> {
        self.0.resolve_address(address)
    }

    fn get_actor_code_cid(
        &self,
        addr: &fvm_shared::address::Address,
    ) -> fvm::kernel::Result<Option<cid_orig::Cid>> {
        self.0.get_actor_code_cid(addr)
    }

    fn new_actor_address(&mut self) -> fvm::kernel::Result<fvm_shared::address::Address> {
        self.0.new_actor_address()
    }

    fn create_actor(
        &mut self,
        code_id: cid_orig::Cid,
        actor_id: ActorID,
    ) -> fvm::kernel::Result<()> {
        self.0.create_actor(code_id, actor_id)
    }
}
impl<DB: BlockStore> fvm::kernel::BlockOps for ForestKernel<DB> {
    fn block_open(&mut self, cid: &cid_orig::Cid) -> fvm::kernel::Result<(BlockId, BlockStat)> {
        self.0.block_open(cid)
    }

    fn block_create(&mut self, codec: u64, data: &[u8]) -> fvm::kernel::Result<BlockId> {
        self.0.block_create(codec, data)
    }

    fn block_link(
        &mut self,
        id: BlockId,
        hash_fun: u64,
        hash_len: u32,
    ) -> fvm::kernel::Result<cid_orig::Cid> {
        self.0.block_link(id, hash_fun, hash_len)
    }

    fn block_read(&self, id: BlockId, offset: u32, buf: &mut [u8]) -> fvm::kernel::Result<u32> {
        self.0.block_read(id, offset, buf)
    }

    fn block_stat(&self, id: BlockId) -> fvm::kernel::Result<BlockStat> {
        self.0.block_stat(id)
    }

    fn block_get(&self, id: BlockId) -> fvm::kernel::Result<(u64, Vec<u8>)> {
        self.0.block_get(id)
    }
}
impl<DB: BlockStore> fvm::kernel::CircSupplyOps for ForestKernel<DB> {
    fn total_fil_circ_supply(&self) -> fvm::kernel::Result<TokenAmount> {
        self.0.total_fil_circ_supply()
    }
}
impl<DB: BlockStore> fvm::kernel::CryptoOps for ForestKernel<DB> {
    // forwarded
    fn hash_blake2b(&mut self, data: &[u8]) -> fvm::kernel::Result<[u8; 32]> {
        self.0.hash_blake2b(data)
    }

    // forwarded
    fn compute_unsealed_sector_cid(
        &mut self,
        proof_type: RegisteredSealProof,
        pieces: &[PieceInfo],
    ) -> fvm::kernel::Result<cid_orig::Cid> {
        self.0.compute_unsealed_sector_cid(proof_type, pieces)
    }

    // forwarded
    fn verify_signature(
        &mut self,
        signature: &Signature,
        signer: &fvm_shared::address::Address,
        plaintext: &[u8],
    ) -> fvm::kernel::Result<bool> {
        self.0.verify_signature(signature, signer, plaintext)
    }

    // NOT forwarded
    fn batch_verify_seals(&mut self, vis: &[SealVerifyInfo]) -> fvm::kernel::Result<Vec<bool>> {
        Ok(vec![true; vis.len()])
    }

    // NOT forwarded
    fn verify_seal(&mut self, _vi: &SealVerifyInfo) -> fvm::kernel::Result<bool> {
        todo!()
        // let charge = self.1.price_list.on_verify_seal(vi);
        // self.0.charge_gas(charge.name, charge.total())?;
        // Ok(true)
    }

    // NOT forwarded
    fn verify_post(&mut self, _vi: &WindowPoStVerifyInfo) -> fvm::kernel::Result<bool> {
        todo!()
        // let charge = self.1.price_list.on_verify_post(vi);
        // self.0.charge_gas(charge.name, charge.total())?;
        // Ok(true)
    }

    // NOT forwarded
    fn verify_consensus_fault(
        &mut self,
        _h1: &[u8],
        _h2: &[u8],
        _extra: &[u8],
    ) -> fvm::kernel::Result<Option<ConsensusFault>> {
        todo!()
        // let charge = self.1.price_list.on_verify_consensus_fault();
        // self.0.charge_gas(charge.name, charge.total())?;
        // // TODO this seems wrong, should probably be parameterized.
        // Ok(None)
    }

    // NOT forwarded
    fn verify_aggregate_seals(
        &mut self,
        _agg: &AggregateSealVerifyProofAndInfos,
    ) -> fvm::kernel::Result<bool> {
        todo!()
        // let charge = self.1.price_list.on_verify_aggregate_seals(agg);
        // self.0.charge_gas(charge.name, charge.total())?;
        // Ok(true)
    }
}
impl<DB: BlockStore> fvm::kernel::DebugOps for ForestKernel<DB> {
    fn log(&self, msg: String) {
        self.0.log(msg)
    }

    fn debug_enabled(&self) -> bool {
        self.0.debug_enabled()
    }
}
impl<DB: BlockStore> fvm::kernel::GasOps for ForestKernel<DB> {
    fn charge_gas(&mut self, name: &str, compute: i64) -> fvm::kernel::Result<()> {
        self.0.charge_gas(name, compute)
    }
}
impl<DB: BlockStore> fvm::kernel::MessageOps for ForestKernel<DB> {
    fn msg_caller(&self) -> ActorID {
        self.0.msg_caller()
    }

    fn msg_receiver(&self) -> ActorID {
        self.0.msg_receiver()
    }

    fn msg_method_number(&self) -> MethodNum {
        self.0.msg_method_number()
    }

    fn msg_value_received(&self) -> TokenAmount {
        self.0.msg_value_received()
    }
}
impl<DB: BlockStore> fvm::kernel::NetworkOps for ForestKernel<DB> {
    fn network_epoch(&self) -> ChainEpoch {
        self.0.network_epoch()
    }

    fn network_version(&self) -> fvm_shared::version::NetworkVersion {
        self.0.network_version()
    }

    fn network_base_fee(&self) -> &TokenAmount {
        self.0.network_base_fee()
    }
}
impl<DB: BlockStore> fvm::kernel::RandomnessOps for ForestKernel<DB> {
    fn get_randomness_from_tickets(
        &self,
        personalization: DomainSeparationTag,
        rand_epoch: ChainEpoch,
        entropy: &[u8],
    ) -> fvm::kernel::Result<[u8; RANDOMNESS_LENGTH]> {
        self.0
            .get_randomness_from_tickets(personalization, rand_epoch, entropy)
    }

    fn get_randomness_from_beacon(
        &self,
        personalization: DomainSeparationTag,
        rand_epoch: ChainEpoch,
        entropy: &[u8],
    ) -> fvm::kernel::Result<[u8; RANDOMNESS_LENGTH]> {
        self.0
            .get_randomness_from_beacon(personalization, rand_epoch, entropy)
    }
}
impl<DB: BlockStore> fvm::kernel::SelfOps for ForestKernel<DB> {
    fn root(&self) -> fvm::kernel::Result<cid_orig::Cid> {
        self.0.root()
    }

    fn set_root(&mut self, root: cid_orig::Cid) -> fvm::kernel::Result<()> {
        self.0.set_root(root)
    }

    fn current_balance(&self) -> fvm::kernel::Result<TokenAmount> {
        self.0.current_balance()
    }

    fn self_destruct(
        &mut self,
        beneficiary: &fvm_shared::address::Address,
    ) -> fvm::kernel::Result<()> {
        self.0.self_destruct(beneficiary)
    }
}
impl<DB: BlockStore> fvm::kernel::SendOps for ForestKernel<DB> {
    fn send(
        &mut self,
        recipient: &fvm_shared::address::Address,
        method: u64,
        params: &fvm_shared::encoding::RawBytes,
        value: &TokenAmount,
    ) -> fvm::kernel::Result<InvocationResult> {
        self.0.send(recipient, method, params, value)
    }
}

/// Interpreter which handles execution of state transitioning messages and returns receipts
/// from the vm execution.
pub struct VM<
    'db,
    'r,
    DB: BlockStore + 'static,
    R,
    N,
    C,
    LB,
    V = FullVerifier,
    P = DefaultNetworkParams,
> {
    state: StateTree<'db, DB>,
    store: &'db DB,
    epoch: ChainEpoch,
    rand: &'r R,
    base_fee: BigInt,
    registered_actors: HashSet<Cid>,
    network_version_getter: N,
    circ_supply_calc: &'r C,
    fvm_executor: fvm::executor::DefaultExecutor<ForestKernel<DB>>,
    lb_state: &'r LB,
    verifier: PhantomData<V>,
    params: PhantomData<P>,
}

impl<'db, 'r, DB, R, N, C, LB, V, P> VM<'db, 'r, DB, R, N, C, LB, V, P>
where
    DB: BlockStore,
    V: ProofVerifier,
    P: NetworkParams,
    R: Rand + Clone + 'static,
    N: Fn(ChainEpoch) -> NetworkVersion,
    C: CircSupplyCalc,
    LB: LookbackStateGetter<'db, DB>,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        root: Cid,
        store: &'db DB,
        store_arc: Arc<DB>,
        epoch: ChainEpoch,
        rand: &'r R,
        base_fee: BigInt,
        network_version_getter: N,
        circ_supply_calc: &'r C,
        lb_state: &'r LB,
    ) -> Result<Self, String> {
        let state = StateTree::new_from_root(store, &root).map_err(|e| e.to_string())?;
        let registered_actors = HashSet::new();
        let engine = Engine::default();
        let base_circ_supply = circ_supply_calc.get_supply(epoch, &state).unwrap();
        let config = Config {
            debug: true,
            ..fvm::Config::default()
        };
        let fvm: fvm::machine::DefaultMachine<FvmStore<DB>, ForestExterns> =
            fvm::machine::DefaultMachine::new(
                config,
                engine,
                epoch,                                    // ChainEpoch,
                base_fee.clone(),                         //base_fee: TokenAmount,
                base_circ_supply,                         // base_circ_supply: TokenAmount,
                fvm_shared::version::NetworkVersion::V14, // network_version: NetworkVersion,
                root.into(),                              //state_root: Cid,
                FvmStore::new(store_arc),
                ForestExterns::new(rand.clone()),
            )
            .unwrap();
        let exec: fvm::executor::DefaultExecutor<ForestKernel<DB>> =
            fvm::executor::DefaultExecutor::new(ForestMachine { machine: fvm });
        Ok(VM {
            network_version_getter,
            state,
            store,
            epoch,
            rand,
            base_fee,
            registered_actors,
            fvm_executor: exec,
            circ_supply_calc,
            lb_state,
            verifier: PhantomData,
            params: PhantomData,
        })
    }

    /// Registers an actor that is not part of the set of default builtin actors by providing the
    /// code cid.
    pub fn register_actor(&mut self, code_cid: Cid) -> bool {
        self.registered_actors.insert(code_cid)
    }

    /// Gets registered actors that are not part of the set of default builtin actors.
    pub fn registered_actors(&self) -> &HashSet<Cid> {
        &self.registered_actors
    }

    /// Flush stores in VM and return state root.
    pub fn flush(&mut self) -> anyhow::Result<Cid> {
        if crate::use_fvm() {
            Ok(self.fvm_executor.flush()?.into())
        } else {
            match self.state.flush() {
                Ok(cid) => Ok(cid),
                Err(err) => anyhow::bail!("{}", err),
            }
        }
    }

    /// Returns a reference to the VM's state tree.
    pub fn state(&self) -> &StateTree<'_, DB> {
        if crate::use_fvm() {
            panic!("State tree reference is not available with FVM.")
        } else {
            &self.state
        }
    }

    fn run_cron(
        &mut self,
        epoch: ChainEpoch,
        callback: Option<&mut impl FnMut(&Cid, &ChainMessage, &ApplyRet) -> Result<(), String>>,
    ) -> Result<(), Box<dyn StdError>> {
        let cron_msg = UnsignedMessage {
            from: **system::ADDRESS,
            to: **cron::ADDRESS,
            // Epoch as sequence is intentional
            sequence: epoch as u64,
            // Arbitrarily large gas limit for cron (matching Lotus value)
            gas_limit: BLOCK_GAS_LIMIT * 10000,
            method_num: cron::Method::EpochTick as u64,
            params: Default::default(),
            value: Default::default(),
            version: Default::default(),
            gas_fee_cap: Default::default(),
            gas_premium: Default::default(),
        };

        let ret = self.apply_implicit_message(&cron_msg);
        if let Some(err) = ret.act_error {
            return Err(format!("failed to apply block cron message: {}", err).into());
        }

        if let Some(callback) = callback {
            callback(
                &(cron_msg.cid()?.into()),
                &ChainMessage::Unsigned(cron_msg),
                &ret,
            )?;
        }
        Ok(())
    }

    /// Flushes the StateTree and perform a state migration if there is a migration at this epoch.
    /// If there is no migration this function will return Ok(None).
    pub fn migrate_state(
        &mut self,
        epoch: ChainEpoch,
        _store: Arc<impl BlockStore + Send + Sync>,
    ) -> Result<Option<Cid>, Box<dyn StdError>> {
        match epoch {
            x if x == UPGRADE_ACTORS_V4_HEIGHT => {
                panic!("Cannot migrate state when using FVM");
            }
            _ => Ok(None),
        }
    }

    /// Apply block messages from a Tipset.
    /// Returns the receipts from the transactions.
    pub fn apply_block_messages(
        &mut self,
        messages: &[BlockMessages],
        parent_epoch: ChainEpoch,
        epoch: ChainEpoch,
        store: std::sync::Arc<impl BlockStore + Send + Sync>,
        mut callback: Option<impl FnMut(&Cid, &ChainMessage, &ApplyRet) -> Result<(), String>>,
    ) -> Result<Vec<MessageReceipt>, Box<dyn StdError>> {
        let mut receipts = Vec::new();
        let mut processed = HashSet::<Cid>::default();

        for i in parent_epoch..epoch {
            if i > parent_epoch {
                // run cron for null rounds if any
                if let Err(e) = self.run_cron(i, callback.as_mut()) {
                    log::error!("Beginning of epoch cron failed to run: {}", e);
                }
            }
            if let Some(_new_state) = self.migrate_state(i, store.clone())? {
                todo!()
                // self.state = StateTree::new_from_root(self.store, &new_state)?
            }
            self.epoch = i + 1;
        }

        for block in messages.iter() {
            let mut penalty = Default::default();
            let mut gas_reward = Default::default();

            let mut process_msg = |msg: &ChainMessage| -> Result<(), Box<dyn StdError>> {
                let cid = msg.cid()?;
                // Ensure no duplicate processing of a message
                if processed.contains(&cid.into()) {
                    return Ok(());
                }
                let ret = self.apply_message(msg)?;

                if let Some(cb) = &mut callback {
                    cb(&cid.into(), msg, &ret)?;
                }

                // Update totals
                gas_reward += &ret.miner_tip;
                penalty += &ret.penalty;
                receipts.push(ret.msg_receipt);

                // Add processed Cid to set of processed messages
                processed.insert(cid.into());
                Ok(())
            };

            for msg in block.messages.iter() {
                process_msg(msg)?;
            }

            // Generate reward transaction for the miner of the block
            let params = Serialized::serialize(AwardBlockRewardParams {
                miner: block.miner,
                penalty,
                gas_reward,
                win_count: block.win_count,
            })?;

            let rew_msg = UnsignedMessage {
                from: **system::ADDRESS,
                to: **reward::ADDRESS,
                method_num: reward::Method::AwardBlockReward as u64,
                params,
                // Epoch as sequence is intentional
                sequence: epoch as u64,
                gas_limit: 1 << 30,
                value: Default::default(),
                version: Default::default(),
                gas_fee_cap: Default::default(),
                gas_premium: Default::default(),
            };

            let ret = self.apply_implicit_message(&rew_msg);
            if let Some(err) = ret.act_error {
                return Err(format!(
                    "failed to apply reward message for miner {}: {}",
                    block.miner, err
                )
                .into());
            }

            // This is more of a sanity check, this should not be able to be hit.
            if ret.msg_receipt.exit_code != ExitCode::Ok {
                return Err(format!(
                    "reward application message failed (exit: {:?})",
                    ret.msg_receipt.exit_code
                )
                .into());
            }

            if let Some(callback) = &mut callback {
                callback(
                    &(rew_msg.cid()?.into()),
                    &ChainMessage::Unsigned(rew_msg),
                    &ret,
                )?;
            }
        }

        if let Err(e) = self.run_cron(epoch, callback.as_mut()) {
            log::error!("End of epoch cron failed to run: {}", e);
        }
        Ok(receipts)
    }

    /// Applies single message through vm and returns result from execution.
    pub fn apply_implicit_message(&mut self, msg: &UnsignedMessage) -> ApplyRet {
        if crate::use_fvm() {
            self.apply_implicit_message_fvm(msg)
        } else {
            self.apply_implicit_message_native(msg)
        }
    }

    fn apply_implicit_message_fvm(&mut self, msg: &UnsignedMessage) -> ApplyRet {
        use fvm::executor::Executor;
        let raw_length = msg.marshal_cbor().expect("encoding error").len();
        // if msg.from.protocol() == fvm_shared::address::Protocol::Secp256k1 {
        //     // 65 bytes signature + 1 byte type + 3 bytes for field info.
        //     raw_length += fvm_shared::crypto::signature::SECP_SIG_LEN + 4;
        // }
        let mut ret = self.fvm_executor
            .execute_message(msg.into(), fvm::executor::ApplyKind::Implicit, raw_length)
            .expect("FIXME: execution failed");
        ret.msg_receipt.gas_used = 0;
        ret.miner_tip = num_bigint::BigInt::zero();
        ret.penalty = num_bigint::BigInt::zero();
        ret.into()
    }

    pub fn apply_implicit_message_native(&mut self, msg: &UnsignedMessage) -> ApplyRet {
        let (return_data, _, act_err) = self.send(msg, None);

        ApplyRet {
            msg_receipt: MessageReceipt {
                return_data,
                exit_code: if let Some(err) = &act_err {
                    err.exit_code()
                } else {
                    ExitCode::Ok
                },
                gas_used: 0,
            },
            act_error: act_err,
            penalty: BigInt::zero(),
            miner_tip: BigInt::zero(),
        }
    }

    /// Applies the state transition for a single message.
    /// Returns ApplyRet structure which contains the message receipt and some meta data.
    pub fn apply_message(&mut self, msg: &ChainMessage) -> Result<ApplyRet, String> {
        if crate::use_fvm() {
            self.apply_message_fvm(msg)
        } else {
            self.apply_message_native(msg)
        }
    }

    fn apply_message_fvm(&mut self, msg: &ChainMessage) -> Result<ApplyRet, String> {
        check_message(msg.message())?;

        use fvm::executor::Executor;
        let unsigned = msg.message();
        let mut raw_length = unsigned.marshal_cbor().expect("encoding error").len();
        if unsigned.from.protocol() == fvm_shared::address::Protocol::Secp256k1 {
            // 65 bytes signature + 1 byte type + 3 bytes for field info.
            raw_length += fvm_shared::crypto::signature::SECP_SIG_LEN + 4;
        }
        match self.fvm_executor.execute_message(
            unsigned.into(),
            fvm::executor::ApplyKind::Explicit,
            raw_length,
        ) {
            Ok(ret) => Ok(ret.into()),
            Err(e) => Err(format!("{:?}", e)),
        }
    }

    fn apply_message_native(&mut self, msg: &ChainMessage) -> Result<ApplyRet, String> {
        check_message(msg.message())?;

        let pl = price_list_by_epoch(self.epoch);
        let ser_msg = msg.marshal_cbor().map_err(|e| e.to_string())?;
        let msg_gas_cost = pl.on_chain_message(ser_msg.len());
        let cost_total = msg_gas_cost.total();

        // Verify the cost of the message is not over the message gas limit.
        if cost_total > msg.gas_limit() {
            return Ok(ApplyRet {
                msg_receipt: MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrOutOfGas,
                    gas_used: 0,
                },
                act_error: Some(vm::actor_error!(SysErrOutOfGas;
                    "Out of gas ({} > {})", cost_total, msg.gas_limit())),
                penalty: &self.base_fee * cost_total,
                miner_tip: BigInt::zero(),
            });
        }

        // Load from actor state.
        let miner_penalty_amount = &self.base_fee * msg.gas_limit();
        let from_act = match self.state.get_actor(msg.from()) {
            Ok(Some(from_act)) => from_act,
            Ok(None) => {
                return Ok(ApplyRet {
                    msg_receipt: MessageReceipt {
                        return_data: Serialized::default(),
                        exit_code: ExitCode::SysErrSenderInvalid,
                        gas_used: 0,
                    },
                    penalty: miner_penalty_amount,
                    act_error: Some(vm::actor_error!(SysErrSenderInvalid; "Sender invalid")),
                    miner_tip: 0.into(),
                });
            }
            Err(e) => {
                println!("sender invalid {}", e);
                return Ok(ApplyRet {
                    msg_receipt: MessageReceipt {
                        return_data: Serialized::default(),
                        exit_code: ExitCode::SysErrSenderInvalid,
                        gas_used: 0,
                    },
                    penalty: miner_penalty_amount,
                    act_error: Some(vm::actor_error!(SysErrSenderInvalid; "Sender invalid")),
                    miner_tip: 0.into(),
                });
            }
        };

        // If from actor is not an account actor, return error.
        #[cfg(not(test_vectors))]
        if !actor::is_account_actor(&from_act.code) {
            return Ok(ApplyRet {
                msg_receipt: MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrSenderInvalid,
                    gas_used: 0,
                },
                penalty: miner_penalty_amount,
                act_error: Some(
                    vm::actor_error!(SysErrSenderInvalid; "send not from account actor"),
                ),
                miner_tip: 0.into(),
            });
        };

        // Check sequence is correct
        if msg.sequence() != from_act.sequence {
            return Ok(ApplyRet {
                msg_receipt: MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrSenderStateInvalid,
                    gas_used: 0,
                },
                penalty: miner_penalty_amount,
                act_error: Some(vm::actor_error!(SysErrSenderStateInvalid;
                    "actor sequence invalid: {} != {}", msg.sequence(), from_act.sequence)),
                miner_tip: 0.into(),
            });
        };

        // Ensure from actor has enough balance to cover the gas cost of the message.
        let gas_cost: TokenAmount = msg.gas_fee_cap() * msg.gas_limit();
        if from_act.balance < gas_cost {
            return Ok(ApplyRet {
                msg_receipt: MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrSenderStateInvalid,
                    gas_used: 0,
                },
                penalty: miner_penalty_amount,
                act_error: Some(vm::actor_error!(SysErrSenderStateInvalid;
                    "actor balance less than needed: {} < {}", from_act.balance, gas_cost)),
                miner_tip: 0.into(),
            });
        };

        // Deduct gas cost and increment sequence
        self.state
            .mutate_actor(msg.from(), |act| {
                act.deduct_funds(&gas_cost)?;
                act.sequence += 1;
                Ok(())
            })
            .map_err(|e| e.to_string())?;

        let send_clo = || -> Result<ApplyRet, String> {
            self.state.snapshot()?;

            // Perform transaction
            let (mut ret_data, rt, mut act_err) = self.send(msg.message(), Some(msg_gas_cost));
            if let Some(err) = &act_err {
                if err.is_fatal() {
                    return Err(format!(
                        "[from={}, to={}, seq={}, m={}, h={}] fatal error: {}",
                        msg.from(),
                        msg.to(),
                        msg.sequence(),
                        msg.method_num(),
                        self.epoch,
                        err
                    ));
                } else {
                    debug!(
                        "[from={}, to={}, seq={}, m={}] send error: {}",
                        msg.from(),
                        msg.to(),
                        msg.sequence(),
                        msg.method_num(),
                        err
                    );
                    if !ret_data.is_empty() {
                        return Err(format!(
                            "message invocation errored, but had a return value anyway: {}",
                            err
                        ));
                    }
                }
            }

            let gas_used = if let Some(mut rt) = rt {
                if !ret_data.is_empty() {
                    if let Err(e) =
                        rt.charge_gas(rt.price_list().on_chain_return_value(ret_data.len()))
                    {
                        act_err = Some(e);
                        ret_data = Serialized::default();
                    }
                }
                if rt.gas_used() < 0 {
                    0
                } else {
                    rt.gas_used()
                }
            } else {
                return Err(format!("send returned None runtime: {:?}", act_err));
            };

            let err_code = if let Some(err) = &act_err {
                if !err.is_ok() {
                    // Revert all state changes on error.
                    self.state.revert_to_snapshot()?;
                }
                err.exit_code()
            } else {
                ExitCode::Ok
            };

            let should_burn = self
                .should_burn(self.state(), msg, err_code)
                .map_err(|e| format!("failed to decide whether to burn: {}", e))?;

            let GasOutputs {
                base_fee_burn,
                miner_tip,
                over_estimation_burn,
                refund,
                miner_penalty,
                ..
            } = compute_gas_outputs(
                gas_used,
                msg.gas_limit(),
                &self.base_fee,
                msg.gas_fee_cap(),
                msg.gas_premium().clone(),
                should_burn,
            );

            let mut transfer_to_actor = |addr: &Address, amt: &TokenAmount| -> Result<(), String> {
                if amt.sign() == Sign::Minus {
                    return Err("attempted to transfer negative value into actor".into());
                }
                if amt.is_zero() {
                    return Ok(());
                }

                self.state
                    .mutate_actor(addr, |act| {
                        act.deposit_funds(amt);
                        Ok(())
                    })
                    .map_err(|e| e.to_string())?;
                Ok(())
            };

            transfer_to_actor(&*BURNT_FUNDS_ACTOR_ADDR, &base_fee_burn)?;

            transfer_to_actor(&**reward::ADDRESS, &miner_tip)?;

            transfer_to_actor(&*BURNT_FUNDS_ACTOR_ADDR, &over_estimation_burn)?;

            // refund unused gas
            transfer_to_actor(msg.from(), &refund)?;

            if &base_fee_burn + over_estimation_burn + &refund + &miner_tip != gas_cost {
                // Sanity check. This could be a fatal error.
                return Err("Gas handling math is wrong".to_owned());
            }

            Ok(ApplyRet {
                msg_receipt: MessageReceipt {
                    return_data: ret_data,
                    exit_code: err_code,
                    gas_used,
                },
                penalty: miner_penalty,
                act_error: act_err,
                miner_tip,
            })
        };

        let res = send_clo();
        self.state.clear_snapshot()?;
        res
    }

    /// Instantiates a new Runtime, and calls vm_send to do the execution.
    #[allow(clippy::type_complexity)]
    fn send(
        &mut self,
        msg: &UnsignedMessage,
        gas_cost: Option<GasCharge>,
    ) -> (
        Serialized,
        Option<DefaultRuntime<'db, '_, DB, R, C, LB, V, P>>,
        Option<ActorError>,
    ) {
        let res = DefaultRuntime::new(
            (self.network_version_getter)(self.epoch),
            &mut self.state,
            self.store,
            0,
            self.base_fee.clone(),
            msg,
            self.epoch,
            *msg.from(),
            msg.sequence(),
            0,
            0,
            self.rand,
            &self.registered_actors,
            self.circ_supply_calc,
            self.lb_state,
        );

        match res {
            Ok(rt) => match rt.send(msg, gas_cost) {
                Ok(ser) => (ser, Some(rt), None),
                Err(actor_err) => (Serialized::default(), Some(rt), Some(actor_err)),
            },
            Err(e) => (Serialized::default(), None, Some(e)),
        }
    }

    fn should_burn(
        &self,
        st: &StateTree<DB>,
        msg: &ChainMessage,
        exit_code: ExitCode,
    ) -> Result<bool, Box<dyn StdError>> {
        if self.epoch <= UPGRADE_ACTORS_V4_HEIGHT {
            // Check to see if we should burn funds. We avoid burning on successful
            // window post. This won't catch _indirect_ window post calls, but this
            // is the best we can get for now.
            if self.epoch > UPGRADE_CLAUS_HEIGHT
                && exit_code.is_success()
                && msg.method_num() == miner::Method::SubmitWindowedPoSt as u64
            {
                // Ok, we've checked the _method_, but we still need to check
                // the target actor.
                let to_actor = st.get_actor(msg.to())?;

                if let Some(actor) = to_actor {
                    if actor::is_miner_actor(&actor.code) {
                        // This is a storage miner and processed a window post, remove burn
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }
}

// // Performs network version 12 / actors v4 state migration
// fn run_nv12_migration(
//     store: Arc<impl BlockStore + Send + Sync>,
//     prev_state: Cid,
//     epoch: i64,
// ) -> Result<Cid, Box<dyn StdError>> {
//     let mut migration = state_migration::StateMigration::new();
//     // Initialize the map with a default set of no-op migrations (nil_migrator).
//     // nv12 migration involves only the miner actor.
//     migration.set_nil_migrations();
//     let (v4_miner_actor_cid, v3_miner_actor_cid) =
//         (*actorv4::MINER_ACTOR_CODE_ID, *actorv3::MINER_ACTOR_CODE_ID);
//     let store_ref = store.clone();
//     let actors_in = StateTree::new_from_root(&*store_ref, &prev_state)
//         .map_err(|e| state_migration::MigrationError::StateTreeCreation(e.to_string()))?;
//     let actors_out = StateTree::new(&*store_ref, StateTreeVersion::V3)
//         .map_err(|e| state_migration::MigrationError::StateTreeCreation(e.to_string()))?;
//     migration.add_migrator(
//         v3_miner_actor_cid,
//         state_migration::nv12::miner_migrator_v4(v4_miner_actor_cid),
//     );
//     let new_state = migration.migrate_state_tree(store, epoch, actors_in, actors_out)?;
//     Ok(new_state)
// }

#[derive(Clone, Default)]
struct GasOutputs {
    base_fee_burn: TokenAmount,
    over_estimation_burn: TokenAmount,
    miner_penalty: TokenAmount,
    miner_tip: TokenAmount,
    refund: TokenAmount,

    gas_refund: i64,
    gas_burned: i64,
}

fn compute_gas_outputs(
    gas_used: i64,
    gas_limit: i64,
    base_fee: &TokenAmount,
    fee_cap: &TokenAmount,
    gas_premium: TokenAmount,
    charge_network_fee: bool,
) -> GasOutputs {
    let mut base_fee_to_pay = base_fee;
    let mut out = GasOutputs::default();

    if base_fee > fee_cap {
        base_fee_to_pay = fee_cap;
        out.miner_penalty = (base_fee - fee_cap) * gas_used
    }

    // If charge network fee is disabled just skip computing the base fee burn.
    // This is part of the temporary fix with Claus fork.
    if charge_network_fee {
        out.base_fee_burn = base_fee_to_pay * gas_used;
    }

    let mut miner_tip = gas_premium;
    if &(base_fee_to_pay + &miner_tip) > fee_cap {
        miner_tip = fee_cap - base_fee_to_pay;
    }
    out.miner_tip = &miner_tip * gas_limit;

    let (out_gas_refund, out_gas_burned) = compute_gas_overestimation_burn(gas_used, gas_limit);
    out.gas_refund = out_gas_refund;
    out.gas_burned = out_gas_burned;

    if out.gas_burned != 0 {
        out.over_estimation_burn = base_fee_to_pay * out.gas_burned;
        out.miner_penalty += (base_fee - base_fee_to_pay) * out.gas_burned;
    }
    let required_funds = fee_cap * gas_limit;
    let refund = required_funds - &out.base_fee_burn - &out.miner_tip - &out.over_estimation_burn;
    out.refund = refund;

    out
}

fn compute_gas_overestimation_burn(gas_used: i64, gas_limit: i64) -> (i64, i64) {
    if gas_used == 0 {
        return (0, gas_limit);
    }

    let mut over = gas_limit - (GAS_OVERUSE_NUM * gas_used) / GAS_OVERUSE_DENOM;
    if over < 0 {
        return (gas_limit - gas_used, 0);
    }

    if over > gas_used {
        over = gas_used;
    }

    let mut gas_to_burn: BigInt = (gas_limit - gas_used).into();
    gas_to_burn *= over;
    gas_to_burn /= gas_used;

    let gas_to_burn = i64::try_from(gas_to_burn).unwrap();
    (gas_limit - gas_used - gas_to_burn, gas_to_burn)
}

/// Apply message return data.
#[derive(Clone, Debug)]
pub struct ApplyRet {
    /// Message receipt for the transaction. This data is stored on chain.
    pub msg_receipt: MessageReceipt,
    /// Actor error from the transaction, if one exists.
    pub act_error: Option<ActorError>,
    /// Gas penalty from transaction, if any.
    pub penalty: BigInt,
    /// Tip given to miner from message.
    pub miner_tip: BigInt,
}

impl From<fvm::executor::ApplyRet> for ApplyRet {
    fn from(ret: fvm::executor::ApplyRet) -> Self {
        let fvm::executor::ApplyRet {
            msg_receipt,
            penalty,
            miner_tip,
            failure_info,
        } = ret;
        ApplyRet {
            msg_receipt,
            act_error: failure_info.map(ActorError::from),
            penalty,
            miner_tip,
        }
    }
}

/// Does some basic checks on the Message to see if the fields are valid.
fn check_message(msg: &UnsignedMessage) -> Result<(), &'static str> {
    if msg.gas_limit() == 0 {
        return Err("Message has no gas limit set");
    }
    if msg.gas_limit() < 0 {
        return Err("Message has negative gas limit");
    }

    Ok(())
}
