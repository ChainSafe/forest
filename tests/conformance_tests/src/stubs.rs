// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

pub struct TestRand;
impl Rand for TestRand {
    fn get_chain_randomness(
        &self,
        _: DomainSeparationTag,
        _: ChainEpoch,
        _: &[u8],
    ) -> Result<[u8; 32], Box<dyn StdError>> {
        Ok(*b"i_am_random_____i_am_random_____")
    }
    fn get_beacon_randomness(
        &self,
        _: DomainSeparationTag,
        _: ChainEpoch,
        _: &[u8],
    ) -> Result<[u8; 32], Box<dyn StdError>> {
        Ok(*b"i_am_random_____i_am_random_____")
    }
    fn get_beacon_randomness_looking_forward(
        &self,
        _: DomainSeparationTag,
        _: ChainEpoch,
        _: &[u8],
    ) -> Result<[u8; 32], Box<dyn StdError>> {
        Ok(*b"i_am_random_____i_am_random_____")
    }
    fn get_chain_randomness_looking_forward(
        &self,
        _: DomainSeparationTag,
        _: ChainEpoch,
        _: &[u8],
    ) -> Result<[u8; 32], Box<dyn StdError>> {
        Ok(*b"i_am_random_____i_am_random_____")
    }
}

pub struct TestSyscalls;
impl Syscalls for TestSyscalls {
    fn verify_signature(
        &self,
        _: &Signature,
        _: &Address,
        _: &[u8],
    ) -> Result<(), Box<dyn StdError>> {
        Ok(())
    }
    fn verify_seal(&self, _: &SealVerifyInfo) -> Result<(), Box<dyn StdError>> {
        Ok(())
    }
    fn verify_post(&self, _: &WindowPoStVerifyInfo) -> Result<(), Box<dyn StdError>> {
        Ok(())
    }

    // TODO check if this should be defaulted as well
    fn verify_consensus_fault(
        &self,
        _: &[u8],
        _: &[u8],
        _: &[u8],
    ) -> Result<Option<ConsensusFault>, Box<dyn StdError>> {
        Ok(None)
    }
    fn verify_aggregate_seals(
        &self,
        _: &fil_types::AggregateSealVerifyProofAndInfos,
    ) -> Result<(), Box<dyn StdError>> {
        Ok(())
    }
}
