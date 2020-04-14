use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use crypto::Signature;
use runtime::{ConsensusFault, Syscalls};
use std::cell::RefCell;
use std::rc::Rc;
use vm::{GasTracker, PieceInfo, PoStVerifyInfo, PriceList, RegisteredProof, SealVerifyInfo};

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
        _signature: &Signature,
        _signer: &Address,
        _plaintext: &[u8],
    ) -> Result<(), &'static str> {
        // TODO
        todo!()
    }
    fn hash_blake2b(&self, _data: &[u8]) -> [u8; 32] {
        // TODO
        todo!()
    }
    fn compute_unsealed_sector_cid(
        &self,
        _reg: RegisteredProof,
        _pieces: &[PieceInfo],
    ) -> Result<Cid, &'static str> {
        // TODO
        todo!()
    }
    fn verify_seal(&self, _vi: SealVerifyInfo) -> Result<(), &'static str> {
        // TODO
        todo!()
    }
    fn verify_post(&self, _vi: PoStVerifyInfo) -> Result<(), &'static str> {
        // TODO
        todo!()
    }
    fn verify_consensus_fault(
        &self,
        _h1: &[u8],
        _h2: &[u8],
        _extra: &[u8],
        _earliest: ChainEpoch,
    ) -> Result<ConsensusFault, &'static str> {
        // TODO
        todo!()
    }
}
