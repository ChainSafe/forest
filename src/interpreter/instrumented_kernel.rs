// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::time;

use cid::Cid;
use fvm2::{
    call_manager::CallManager,
    gas::{Gas, PriceList},
    kernel::{
        BlockId, BlockRegistry, BlockStat, DebugOps, GasOps, MessageOps, NetworkOps, RandomnessOps,
        Result, SelfOps, SendOps, SendResult,
    },
};
use fvm_ipld_blockstore::Blockstore;
use fvm_shared2::{
    address::Address, clock::ChainEpoch, consensus::ConsensusFault,
    crypto::signature::SignatureType, econ::TokenAmount, piece::PieceInfo,
    randomness::RANDOMNESS_LENGTH, sector::*, ActorID, MethodNum,
};
use stdext::function_name;

use crate::interpreter::{metrics, ForestMachine};

/// Calls the supplied lambda and updates the corresponding Prometheus metrics -
/// call count and total call duration.
macro_rules! forward_instrumented {
    ($call:expr) => {{
        let stopwatch = time::Instant::now();
        let result = $call();
        let elapsed = stopwatch.elapsed();
        let fn_name = function_name!()
            .rsplit_once(':')
            .expect("could not parse function name")
            .1;

        metrics::KERNEL_OP_COUNT.with_label_values(&[fn_name]).inc();
        metrics::KERNEL_OP_DURATION
            .with_label_values(&[fn_name])
            .inc_by(elapsed.as_nanos() as u64);

        result
    }};
}

/// Instrumented Kernel flavor. Having overhead of additional metrics, it
/// provides general information of its method usage via Prometheus.
pub struct ForestInstrumentedKernel<DB: Blockstore + 'static>(
    fvm2::DefaultKernel<fvm2::call_manager::DefaultCallManager<ForestMachine<DB>>>,
    Option<TokenAmount>,
);

impl<DB: Blockstore> fvm2::Kernel for ForestInstrumentedKernel<DB> {
    type CallManager = fvm2::call_manager::DefaultCallManager<ForestMachine<DB>>;

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
        let circ_supply = mgr.context().circ_supply.clone();
        ForestInstrumentedKernel(
            fvm2::DefaultKernel::new(mgr, blocks, caller, actor_id, method, value_received),
            Some(circ_supply),
        )
    }
}
impl<DB: Blockstore> fvm2::kernel::ActorOps for ForestInstrumentedKernel<DB> {
    fn resolve_address(&self, address: &Address) -> fvm2::kernel::Result<Option<ActorID>> {
        forward_instrumented!(|| self.0.resolve_address(address))
    }

    fn get_actor_code_cid(&self, id: ActorID) -> fvm2::kernel::Result<Option<Cid>> {
        forward_instrumented!(|| self.0.get_actor_code_cid(id))
    }

    fn new_actor_address(&mut self) -> fvm2::kernel::Result<Address> {
        forward_instrumented!(|| self.0.new_actor_address())
    }

    fn create_actor(&mut self, code_id: Cid, actor_id: ActorID) -> fvm2::kernel::Result<()> {
        forward_instrumented!(|| self.0.create_actor(code_id, actor_id))
    }

    fn get_builtin_actor_type(&self, code_cid: &Cid) -> u32 {
        forward_instrumented!(|| self.0.get_builtin_actor_type(code_cid))
    }

    fn get_code_cid_for_type(&self, typ: u32) -> Result<Cid> {
        forward_instrumented!(|| self.0.get_code_cid_for_type(typ))
    }
}
impl<DB: Blockstore> fvm2::kernel::IpldBlockOps for ForestInstrumentedKernel<DB> {
    fn block_open(&mut self, cid: &Cid) -> fvm2::kernel::Result<(BlockId, BlockStat)> {
        forward_instrumented!(|| self.0.block_open(cid))
    }

    fn block_create(&mut self, codec: u64, data: &[u8]) -> fvm2::kernel::Result<BlockId> {
        forward_instrumented!(|| self.0.block_create(codec, data))
    }

    fn block_link(
        &mut self,
        id: BlockId,
        hash_fun: u64,
        hash_len: u32,
    ) -> fvm2::kernel::Result<Cid> {
        forward_instrumented!(|| self.0.block_link(id, hash_fun, hash_len))
    }

    fn block_read(
        &mut self,
        id: BlockId,
        offset: u32,
        buf: &mut [u8],
    ) -> fvm2::kernel::Result<i32> {
        forward_instrumented!(|| self.0.block_read(id, offset, buf))
    }

    fn block_stat(&mut self, id: BlockId) -> fvm2::kernel::Result<BlockStat> {
        forward_instrumented!(|| self.0.block_stat(id))
    }
}
impl<DB: Blockstore> fvm2::kernel::CircSupplyOps for ForestInstrumentedKernel<DB> {
    fn total_fil_circ_supply(&self) -> fvm2::kernel::Result<TokenAmount> {
        match self.1.clone() {
            Some(supply) => Ok(supply),
            None => {
                forward_instrumented!(|| self.0.total_fil_circ_supply())
            }
        }
    }
}

impl<DB: Blockstore> fvm2::kernel::CryptoOps for ForestInstrumentedKernel<DB> {
    fn hash(&mut self, code: u64, data: &[u8]) -> Result<cid::multihash::MultihashGeneric<64>> {
        forward_instrumented!(|| self.0.hash(code, data))
    }

    fn compute_unsealed_sector_cid(
        &mut self,
        proof_type: RegisteredSealProof,
        pieces: &[PieceInfo],
    ) -> Result<Cid> {
        forward_instrumented!(|| self.0.compute_unsealed_sector_cid(proof_type, pieces))
    }

    fn verify_signature(
        &mut self,
        sig_type: SignatureType,
        signature: &[u8],
        signer: &Address,
        plaintext: &[u8],
    ) -> Result<bool> {
        forward_instrumented!(|| self
            .0
            .verify_signature(sig_type, signature, signer, plaintext))
    }

    fn batch_verify_seals(&mut self, vis: &[SealVerifyInfo]) -> Result<Vec<bool>> {
        forward_instrumented!(|| self.0.batch_verify_seals(vis))
    }

    fn verify_seal(&mut self, vi: &SealVerifyInfo) -> Result<bool> {
        forward_instrumented!(|| self.0.verify_seal(vi))
    }

    fn verify_post(&mut self, vi: &WindowPoStVerifyInfo) -> Result<bool> {
        forward_instrumented!(|| self.0.verify_post(vi))
    }

    fn verify_consensus_fault(
        &mut self,
        h1: &[u8],
        h2: &[u8],
        extra: &[u8],
    ) -> Result<Option<ConsensusFault>> {
        forward_instrumented!(|| self.0.verify_consensus_fault(h1, h2, extra))
    }

    fn verify_aggregate_seals(&mut self, agg: &AggregateSealVerifyProofAndInfos) -> Result<bool> {
        forward_instrumented!(|| self.0.verify_aggregate_seals(agg))
    }

    fn verify_replica_update(&mut self, replica: &ReplicaUpdateInfo) -> Result<bool> {
        forward_instrumented!(|| self.0.verify_replica_update(replica))
    }

    fn recover_secp_public_key(
        &mut self,
        hash: &[u8; fvm_shared2::crypto::signature::SECP_SIG_MESSAGE_HASH_SIZE],
        signature: &[u8; fvm_shared2::crypto::signature::SECP_SIG_LEN],
    ) -> Result<[u8; fvm_shared2::crypto::signature::SECP_PUB_LEN]> {
        forward_instrumented!(|| self.0.recover_secp_public_key(hash, signature))
    }
}
impl<DB: Blockstore> DebugOps for ForestInstrumentedKernel<DB> {
    fn log(&mut self, msg: String) {
        forward_instrumented!(|| self.0.log(msg))
    }

    fn debug_enabled(&self) -> bool {
        forward_instrumented!(|| self.0.debug_enabled())
    }

    fn store_artifact(&self, name: &str, data: &[u8]) -> Result<()> {
        forward_instrumented!(|| self.0.store_artifact(name, data))
    }
}
impl<DB: Blockstore> GasOps for ForestInstrumentedKernel<DB> {
    /// Returns the gas used by the transaction so far.
    fn gas_used(&self) -> Gas {
        forward_instrumented!(|| self.0.gas_used())
    }

    /// Returns the remaining gas for the transaction.
    fn gas_available(&self) -> Gas {
        forward_instrumented!(|| self.0.gas_available())
    }

    /// `charge_gas` charges specified amount of `gas` for execution.
    /// `name` provides information about gas charging point.
    fn charge_gas(&mut self, name: &str, compute: Gas) -> Result<()> {
        forward_instrumented!(|| self.0.charge_gas(name, compute))
    }

    /// Returns the currently active gas price list.
    fn price_list(&self) -> &PriceList {
        forward_instrumented!(|| self.0.price_list())
    }
}
impl<DB: Blockstore> MessageOps for ForestInstrumentedKernel<DB> {
    fn msg_caller(&self) -> ActorID {
        forward_instrumented!(|| self.0.msg_caller())
    }

    fn msg_receiver(&self) -> ActorID {
        forward_instrumented!(|| self.0.msg_receiver())
    }

    fn msg_method_number(&self) -> MethodNum {
        forward_instrumented!(|| self.0.msg_method_number())
    }

    fn msg_value_received(&self) -> TokenAmount {
        forward_instrumented!(|| self.0.msg_value_received())
    }
}
impl<DB: Blockstore> NetworkOps for ForestInstrumentedKernel<DB> {
    fn network_epoch(&self) -> ChainEpoch {
        forward_instrumented!(|| self.0.network_epoch())
    }

    fn network_version(&self) -> crate::shim::version::NetworkVersion_v2 {
        forward_instrumented!(|| self.0.network_version())
    }

    fn network_base_fee(&self) -> &TokenAmount {
        forward_instrumented!(|| self.0.network_base_fee())
    }
}
impl<DB: Blockstore> RandomnessOps for ForestInstrumentedKernel<DB> {
    fn get_randomness_from_tickets(
        &mut self,
        personalization: i64,
        rand_epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; RANDOMNESS_LENGTH]> {
        forward_instrumented!(|| self.0.get_randomness_from_tickets(
            personalization,
            rand_epoch,
            entropy
        ))
    }

    fn get_randomness_from_beacon(
        &mut self,
        personalization: i64,
        rand_epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; RANDOMNESS_LENGTH]> {
        forward_instrumented!(|| self.0.get_randomness_from_beacon(
            personalization,
            rand_epoch,
            entropy
        ))
    }
}
impl<DB: Blockstore> SelfOps for ForestInstrumentedKernel<DB> {
    fn root(&self) -> Result<Cid> {
        forward_instrumented!(|| self.0.root())
    }

    fn set_root(&mut self, root: Cid) -> Result<()> {
        forward_instrumented!(|| self.0.set_root(root))
    }

    fn current_balance(&self) -> Result<TokenAmount> {
        forward_instrumented!(|| self.0.current_balance())
    }

    fn self_destruct(&mut self, beneficiary: &Address) -> Result<()> {
        forward_instrumented!(|| self.0.self_destruct(beneficiary))
    }
}
impl<DB: Blockstore> SendOps for ForestInstrumentedKernel<DB> {
    fn send(
        &mut self,
        recipient: &Address,
        method: u64,
        params: BlockId,
        value: &TokenAmount,
    ) -> Result<SendResult> {
        forward_instrumented!(|| self.0.send(recipient, method, params, value))
    }
}
