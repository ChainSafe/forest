use crate::base64_bytes;

use address::Address;
use clock::ChainEpoch;
use crypto::{DomainSeparationTag, Signature};
use encoding::tuple::*;
use fil_types::{SealVerifyInfo, WindowPoStVerifyInfo};
use interpreter::Rand;
use runtime::{ConsensusFault, Syscalls};
use serde::{Deserialize, Deserializer};

use std::error::Error as StdError;

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RandomnessKind {
    Beacon,
    Chain,
}

#[derive(Debug, Deserialize)]
pub struct RandomnessMatch {
    pub on: RandomnessRule,
    #[serde(with = "base64_bytes")]
    pub ret: Vec<u8>,
}

#[derive(Debug, Deserialize_tuple, PartialEq)]
pub struct RandomnessRule {
    pub kind: RandomnessKind,
    pub dst: DomainSeparationTag,
    pub epoch: ChainEpoch,
    #[serde(with = "base64_bytes")]
    pub entropy: Vec<u8>,
}

pub struct ReplayingRand<'a> {
    pub recorded: &'a [RandomnessMatch],
    pub fallback: TestRand,
}

impl<'a> ReplayingRand<'a> {
    pub fn new(recorded: &'a [RandomnessMatch]) -> Self {
        Self {
            recorded,
            fallback: TestRand,
        }
    }

    pub fn matches(&self, requested: RandomnessRule) -> Option<[u8; 32]> {
        for other in self.recorded {
            if other.on == requested {
                let mut randomness = [0u8; 32];
                randomness.copy_from_slice(&other.ret);
                return Some(randomness);
            }
        }
        None
    }
}

impl Rand for ReplayingRand<'_> {
    fn get_chain_randomness(
        &self,
        dst: DomainSeparationTag,
        epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn StdError>> {
        let rule = RandomnessRule {
            kind: RandomnessKind::Chain,
            dst,
            epoch,
            entropy: entropy.to_vec(),
        };
        if let Some(bz) = self.matches(rule) {
            Ok(bz)
        } else {
            self.fallback.get_chain_randomness(dst, epoch, entropy)
        }
    }
    fn get_beacon_randomness(
        &self,
        dst: DomainSeparationTag,
        epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn StdError>> {
        let rule = RandomnessRule {
            kind: RandomnessKind::Beacon,
            dst,
            epoch,
            entropy: entropy.to_vec(),
        };
        if let Some(bz) = self.matches(rule) {
            Ok(bz)
        } else {
            self.fallback.get_beacon_randomness(dst, epoch, entropy)
        }
    }
    // TODO: Check if this is going to be correct for when we integrate v5 Actors test vectors
    fn get_beacon_randomness_looking_forward(
        &self,
        dst: DomainSeparationTag,
        epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn StdError>> {
        let rule = RandomnessRule {
            kind: RandomnessKind::Beacon,
            dst,
            epoch,
            entropy: entropy.to_vec(),
        };
        if let Some(bz) = self.matches(rule) {
            Ok(bz)
        } else {
            self.fallback.get_beacon_randomness(dst, epoch, entropy)
        }
    }
    // TODO: Check if this is going to be correct for when we integrate v5 Actors test vectors
    fn get_chain_randomness_looking_forward(
        &self,
        dst: DomainSeparationTag,
        epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Result<[u8; 32], Box<dyn StdError>> {
        let rule = RandomnessRule {
            kind: RandomnessKind::Chain,
            dst,
            epoch,
            entropy: entropy.to_vec(),
        };
        if let Some(bz) = self.matches(rule) {
            Ok(bz)
        } else {
            self.fallback.get_chain_randomness(dst, epoch, entropy)
        }
    }
}

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
