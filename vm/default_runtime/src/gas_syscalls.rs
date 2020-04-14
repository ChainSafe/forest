use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use crypto::Signature;
use runtime::{ConsensusFault, Syscalls};
use std::cell::RefCell;
use std::rc::Rc;
use vm::{
    ActorError, GasTracker, PieceInfo, PoStVerifyInfo, PriceList, RegisteredProof, SealVerifyInfo,
};

/// Syscall wrapper to charge gas on syscalls
pub(crate) struct GasSyscalls<S: Copy> {
    pub price_list: PriceList,
    pub gas: Rc<RefCell<GasTracker>>,
    pub syscalls: S,
}

impl<S> Syscalls for GasSyscalls<S>
where
    S: Syscalls + Copy,
{
    fn verify_signature(
        &self,
        signature: &Signature,
        signer: &Address,
        plaintext: &[u8],
    ) -> Result<(), ActorError> {
        self.gas
            .borrow_mut()
            .charge_gas(
                self.price_list
                    .on_verify_signature(signature.signature_type(), plaintext.len()),
            )
            .unwrap();
        self.syscalls.verify_signature(signature, signer, plaintext)
    }
    fn hash_blake2b(&self, data: &[u8]) -> Result<[u8; 32], ActorError> {
        self.gas
            .borrow_mut()
            .charge_gas(self.price_list.on_hashing(data.len()))
            .unwrap();
        self.syscalls.hash_blake2b(data)
    }
    fn compute_unsealed_sector_cid(
        &self,
        reg: RegisteredProof,
        pieces: &[PieceInfo],
    ) -> Result<Cid, ActorError> {
        self.gas
            .borrow_mut()
            .charge_gas(self.price_list.on_compute_unsealed_sector_cid(reg, pieces))
            .unwrap();
        self.syscalls.compute_unsealed_sector_cid(reg, pieces)
    }
    fn verify_seal(&self, vi: &SealVerifyInfo) -> Result<(), ActorError> {
        self.gas
            .borrow_mut()
            .charge_gas(self.price_list.on_verify_seal(vi))
            .unwrap();
        self.syscalls.verify_seal(vi)
    }
    fn verify_post(&self, vi: &PoStVerifyInfo) -> Result<(), ActorError> {
        self.gas
            .borrow_mut()
            .charge_gas(self.price_list.on_verify_post(vi))
            .unwrap();
        self.syscalls.verify_post(vi)
    }
    fn verify_consensus_fault(
        &self,
        h1: &[u8],
        h2: &[u8],
        extra: &[u8],
        earliest: ChainEpoch,
    ) -> Result<ConsensusFault, ActorError> {
        self.gas
            .borrow_mut()
            .charge_gas(self.price_list.on_verify_consensus_fault())
            .unwrap();
        self.syscalls
            .verify_consensus_fault(h1, h2, extra, earliest)
    }
}
