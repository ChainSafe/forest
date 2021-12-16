// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

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
    fn get_chain_randomness_v1(
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
            self.fallback.get_chain_randomness_v1(dst, epoch, entropy)
        }
    }
    fn get_beacon_randomness_v1(
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
            self.fallback.get_beacon_randomness_v1(dst, epoch, entropy)
        }
    }
    // TODO: Check if this is going to be correct for when we integrate v5 Actors test vectors
    fn get_beacon_randomness_v2(
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
            self.fallback.get_beacon_randomness_v2(dst, epoch, entropy)
        }
    }
    // TODO: Check if this is going to be correct for when we integrate v5 Actors test vectors
    fn get_chain_randomness_v2(
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
            self.fallback.get_chain_randomness_v2(dst, epoch, entropy)
        }
    }

    fn get_beacon_randomness_v3(
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
            self.fallback.get_beacon_randomness_v3(dst, epoch, entropy)
        }
    }
}
