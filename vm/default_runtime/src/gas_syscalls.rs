// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

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

#[cfg(test)]
mod tests {
    use super::*;
    use runtime::{ConsensusFault, ConsensusFaultType, Syscalls};

    #[derive(Copy, Debug, Clone)]
    struct TestSyscalls;
    impl Syscalls for TestSyscalls {
        fn verify_signature(
            &self,
            _signature: &Signature,
            _signer: &Address,
            _plaintext: &[u8],
        ) -> Result<(), ActorError> {
            Ok(())
        }
        fn hash_blake2b(&self, _data: &[u8]) -> Result<[u8; 32], ActorError> {
            Ok([0u8; 32])
        }
        fn compute_unsealed_sector_cid(
            &self,
            _reg: RegisteredProof,
            _pieces: &[PieceInfo],
        ) -> Result<Cid, ActorError> {
            Ok(Default::default())
        }
        fn verify_seal(&self, _vi: &SealVerifyInfo) -> Result<(), ActorError> {
            Ok(Default::default())
        }
        fn verify_post(&self, _vi: &PoStVerifyInfo) -> Result<(), ActorError> {
            Ok(Default::default())
        }
        fn verify_consensus_fault(
            &self,
            _h1: &[u8],
            _h2: &[u8],
            _extra: &[u8],
            _earliest: ChainEpoch,
        ) -> Result<ConsensusFault, ActorError> {
            Ok(ConsensusFault {
                target: Address::new_id(0).unwrap(),
                epoch: 0,
                fault_type: ConsensusFaultType::DoubleForkMining,
            })
        }
    }

    #[test]
    fn gas_syscalls() {
        let gsys = GasSyscalls {
            price_list: PriceList {
                on_chain_message_base: 1,
                on_chain_message_per_byte: 1,
                on_chain_return_value_per_byte: 1,
                hashing_base: 1,
                hashing_per_byte: 1,
                compute_unsealed_sector_cid_base: 1,
                verify_seal_base: 1,
                verify_post_base: 1,
                verify_consensus_fault: 1,
                ..Default::default()
            },
            gas: Rc::new(RefCell::new(GasTracker::new(20, 0))),
            syscalls: TestSyscalls,
        };

        assert_eq!(gsys.gas.borrow().gas_used(), 0);
        gsys.verify_signature(&Default::default(), &Default::default(), &[0u8])
            .unwrap();
        assert_eq!(gsys.gas.borrow().gas_used(), 5);

        gsys.hash_blake2b(&[0u8]).unwrap();
        assert_eq!(gsys.gas.borrow().gas_used(), 7);

        gsys.compute_unsealed_sector_cid(Default::default(), &[])
            .unwrap();
        assert_eq!(gsys.gas.borrow().gas_used(), 8);

        gsys.verify_seal(&Default::default()).unwrap();
        assert_eq!(gsys.gas.borrow().gas_used(), 9);

        gsys.verify_post(&Default::default()).unwrap();
        assert_eq!(gsys.gas.borrow().gas_used(), 10);

        gsys.verify_consensus_fault(&[], &[], &[], 0).unwrap();
        assert_eq!(gsys.gas.borrow().gas_used(), 11);
    }
}
