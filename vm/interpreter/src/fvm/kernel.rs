// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::ForestMachine;
use forest_cid::Cid;
use forest_vm::TokenAmount;
use fvm::call_manager::*;
use fvm::gas::{Gas, PriceList};
use fvm::kernel::BlockRegistry;
use fvm::kernel::Result;
use fvm::kernel::{
    BlockId, BlockStat, DebugOps, GasOps, MessageOps, NetworkOps, RandomnessOps, SelfOps, SendOps,
    SendResult,
};
use fvm_shared::address::Address;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::consensus::ConsensusFault;
use fvm_shared::crypto::signature::SignatureType;
use fvm_shared::piece::PieceInfo;
use fvm_shared::randomness::RANDOMNESS_LENGTH;
use fvm_shared::sector::*;
use fvm_shared::{ActorID, MethodNum};
use ipld_blockstore::BlockStore;

pub struct ForestKernel<DB: BlockStore + 'static>(
    fvm::DefaultKernel<fvm::call_manager::DefaultCallManager<ForestMachine<DB>>>,
    Option<TokenAmount>,
);

impl<DB: BlockStore> fvm::Kernel for ForestKernel<DB> {
    type CallManager = fvm::call_manager::DefaultCallManager<ForestMachine<DB>>;

    fn into_inner(self) -> (Self::CallManager, BlockRegistry) {
        self.0.into_inner()
    }

    fn new(
        mgr: Self::CallManager,
        blocks: BlockRegistry,
        caller: ActorID,
        actor_id: ActorID,
        method: MethodNum,
        value_received: TokenAmount,
    ) -> Self
    where
        Self: Sized,
    {
        let circ_supply = mgr.machine().circ_supply.clone();
        ForestKernel(
            fvm::DefaultKernel::new(mgr, blocks, caller, actor_id, method, value_received),
            circ_supply,
        )
    }
}
impl<DB: BlockStore> fvm::kernel::ActorOps for ForestKernel<DB> {
    fn resolve_address(&self, address: &Address) -> fvm::kernel::Result<Option<ActorID>> {
        self.0.resolve_address(address)
    }

    fn get_actor_code_cid(&self, id: ActorID) -> fvm::kernel::Result<Option<Cid>> {
        self.0.get_actor_code_cid(id)
    }

    fn new_actor_address(&mut self) -> fvm::kernel::Result<Address> {
        self.0.new_actor_address()
    }

    fn create_actor(&mut self, code_id: Cid, actor_id: ActorID) -> fvm::kernel::Result<()> {
        self.0.create_actor(code_id, actor_id)
    }

    fn get_builtin_actor_type(&self, code_cid: &Cid) -> Option<fvm_shared::actor::builtin::Type> {
        self.0.get_builtin_actor_type(code_cid)
    }

    fn get_code_cid_for_type(&self, typ: fvm_shared::actor::builtin::Type) -> Result<Cid> {
        self.0.get_code_cid_for_type(typ)
    }
}
impl<DB: BlockStore> fvm::kernel::IpldBlockOps for ForestKernel<DB> {
    fn block_open(&mut self, cid: &Cid) -> fvm::kernel::Result<(BlockId, BlockStat)> {
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
    ) -> fvm::kernel::Result<Cid> {
        self.0.block_link(id, hash_fun, hash_len)
    }

    fn block_read(&mut self, id: BlockId, offset: u32, buf: &mut [u8]) -> fvm::kernel::Result<i32> {
        self.0.block_read(id, offset, buf)
    }

    fn block_stat(&mut self, id: BlockId) -> fvm::kernel::Result<BlockStat> {
        self.0.block_stat(id)
    }
}
impl<DB: BlockStore> fvm::kernel::CircSupplyOps for ForestKernel<DB> {
    fn total_fil_circ_supply(&self) -> fvm::kernel::Result<TokenAmount> {
        match self.1.clone() {
            Some(supply) => Ok(supply),
            None => self.0.total_fil_circ_supply(),
        }
    }
}
impl<DB: BlockStore> fvm::kernel::CryptoOps for ForestKernel<DB> {
    // forwarded
    fn hash(&mut self, code: u64, data: &[u8]) -> Result<[u8; 32]> {
        self.0.hash(code, data)
    }

    // forwarded
    fn compute_unsealed_sector_cid(
        &mut self,
        proof_type: RegisteredSealProof,
        pieces: &[PieceInfo],
    ) -> Result<Cid> {
        self.0.compute_unsealed_sector_cid(proof_type, pieces)
    }

    // forwarded
    fn verify_signature(
        &mut self,
        sig_type: SignatureType,
        signature: &[u8],
        signer: &Address,
        plaintext: &[u8],
    ) -> Result<bool> {
        self.0
            .verify_signature(sig_type, signature, signer, plaintext)
    }

    // forwarded
    fn batch_verify_seals(&mut self, vis: &[SealVerifyInfo]) -> Result<Vec<bool>> {
        self.0.batch_verify_seals(vis)
    }

    // forwarded
    fn verify_seal(&mut self, vi: &SealVerifyInfo) -> Result<bool> {
        self.0.verify_seal(vi)
    }

    // forwarded
    fn verify_post(&mut self, vi: &WindowPoStVerifyInfo) -> Result<bool> {
        self.0.verify_post(vi)
    }

    // forwarded
    fn verify_consensus_fault(
        &mut self,
        h1: &[u8],
        h2: &[u8],
        extra: &[u8],
    ) -> Result<Option<ConsensusFault>> {
        self.0.verify_consensus_fault(h1, h2, extra)
    }

    // forwarded
    fn verify_aggregate_seals(&mut self, agg: &AggregateSealVerifyProofAndInfos) -> Result<bool> {
        self.0.verify_aggregate_seals(agg)
    }

    // forwarded
    fn verify_replica_update(&mut self, replica: &ReplicaUpdateInfo) -> Result<bool> {
        self.0.verify_replica_update(replica)
    }
}
impl<DB: BlockStore> DebugOps for ForestKernel<DB> {
    fn log(&self, msg: String) {
        self.0.log(msg)
    }

    fn debug_enabled(&self) -> bool {
        self.0.debug_enabled()
    }
}
impl<DB: BlockStore> GasOps for ForestKernel<DB> {
    /// Returns the gas used by the transaction so far.
    fn gas_used(&self) -> Gas {
        self.0.gas_used()
    }

    /// Returns the remaining gas for the transaction.
    fn gas_available(&self) -> Gas {
        self.0.gas_available()
    }

    /// ChargeGas charges specified amount of `gas` for execution.
    /// `name` provides information about gas charging point.
    fn charge_gas(&mut self, name: &str, compute: Gas) -> Result<()> {
        self.0.charge_gas(name, compute)
    }

    /// Returns the currently active gas price list.
    fn price_list(&self) -> &PriceList {
        self.0.price_list()
    }
}
impl<DB: BlockStore> MessageOps for ForestKernel<DB> {
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
impl<DB: BlockStore> NetworkOps for ForestKernel<DB> {
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
impl<DB: BlockStore> RandomnessOps for ForestKernel<DB> {
    fn get_randomness_from_tickets(
        &mut self,
        personalization: i64,
        rand_epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; RANDOMNESS_LENGTH]> {
        self.0
            .get_randomness_from_tickets(personalization, rand_epoch, entropy)
    }

    fn get_randomness_from_beacon(
        &mut self,
        personalization: i64,
        rand_epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; RANDOMNESS_LENGTH]> {
        self.0
            .get_randomness_from_beacon(personalization, rand_epoch, entropy)
    }
}
impl<DB: BlockStore> SelfOps for ForestKernel<DB> {
    fn root(&self) -> Result<Cid> {
        self.0.root()
    }

    fn set_root(&mut self, root: Cid) -> Result<()> {
        self.0.set_root(root)
    }

    fn current_balance(&self) -> Result<TokenAmount> {
        self.0.current_balance()
    }

    fn self_destruct(&mut self, beneficiary: &Address) -> Result<()> {
        self.0.self_destruct(beneficiary)
    }
}
impl<DB: BlockStore> SendOps for ForestKernel<DB> {
    fn send(
        &mut self,
        recipient: &Address,
        method: u64,
        params: BlockId,
        value: &TokenAmount,
    ) -> Result<SendResult> {
        self.0.send(recipient, method, params, value)
    }
}
