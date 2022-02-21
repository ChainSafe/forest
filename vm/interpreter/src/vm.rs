// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Rand;
use actor::{actorv0::reward::AwardBlockRewardParams, cron, reward, system};
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
use networks::UPGRADE_ACTORS_V4_HEIGHT;
use num_bigint::BigInt;
use state_tree::StateTree;
use std::collections::HashSet;
use std::error::Error as StdError;
use std::marker::PhantomData;
use std::sync::Arc;
use vm::{ActorError, ExitCode, Serialized, TokenAmount};

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
pub trait LookbackStateGetter<DB> {
    /// Returns a state tree from the given epoch.
    fn state_lookback(&self, epoch: ChainEpoch) -> Result<StateTree<'_, DB>, Box<dyn StdError>>;
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

use fvm::call_manager::*;
use fvm::kernel::{BlockId, BlockStat};
use fvm_shared::crypto::signature::Signature;
use fvm_shared::piece::PieceInfo;
use fvm_shared::randomness::RANDOMNESS_LENGTH;
use fvm_shared::sector::*;
use fvm_shared::{ActorID, MethodNum};

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
pub struct VM<'db, DB: BlockStore + 'static, V = FullVerifier, P = DefaultNetworkParams> {
    state: StateTree<'db, DB>,
    store: &'db DB,
    epoch: ChainEpoch,
    registered_actors: HashSet<Cid>,
    fvm_executor: fvm::executor::DefaultExecutor<ForestKernel<DB>>,
    verifier: PhantomData<V>,
    params: PhantomData<P>,
}

impl<'db, DB, V, P> VM<'db, DB, V, P>
where
    DB: BlockStore + 'static,
    V: ProofVerifier,
    P: NetworkParams,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        root: Cid,
        store: &'db DB,
        store_arc: Arc<DB>,
        epoch: ChainEpoch,
        rand: impl Rand + Clone + 'static,
        base_fee: BigInt,
        circ_supply_calc: impl CircSupplyCalc,
    ) -> Result<Self, String> {
        // let store = store_arc.as_ref();
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
                base_fee,                                 //base_fee: TokenAmount,
                base_circ_supply,                         // base_circ_supply: TokenAmount,
                fvm_shared::version::NetworkVersion::V14, // network_version: NetworkVersion,
                root.into(),                              //state_root: Cid,
                FvmStore::new(store_arc),
                ForestExterns::new(rand),
            )
            .unwrap();
        let exec: fvm::executor::DefaultExecutor<ForestKernel<DB>> =
            fvm::executor::DefaultExecutor::new(ForestMachine { machine: fvm });
        Ok(VM {
            state,
            store,
            epoch,
            registered_actors,
            // fvm_machine: ForestMachine{ machine: fvm },
            fvm_executor: exec,
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
        Ok(self.fvm_executor.flush()?.into())
        // self.state.flush()
    }

    /// Returns a reference to the VM's state tree.
    pub fn state(&self) -> &StateTree<'_, DB> {
        panic!("State reference is no longer available.")
        // &self.state
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
        use fvm::executor::Executor;
        let mut raw_length = msg.marshal_cbor().expect("encoding error").len();
        if msg.from.protocol() == fvm_shared::address::Protocol::Secp256k1 {
            // 65 bytes signature + 1 byte type + 3 bytes for field info.
            raw_length += fvm_shared::crypto::signature::SECP_SIG_LEN + 4;
        }
        self.fvm_executor
            .execute_message(msg.into(), fvm::executor::ApplyKind::Implicit, raw_length)
            .expect("FIXME: execution failed")
            .into()
    }

    /// Applies the state transition for a single message.
    /// Returns ApplyRet structure which contains the message receipt and some meta data.
    pub fn apply_message(&mut self, msg: &ChainMessage) -> Result<ApplyRet, String> {
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
