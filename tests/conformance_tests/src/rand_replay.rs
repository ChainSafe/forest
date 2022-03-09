// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

#[derive(Clone)]
pub struct ReplayingRand {
    pub recorded: Vec<RandomnessMatch>,
    pub fallback: TestRand,
}

impl<'a> ReplayingRand {
    pub fn new(recorded: &[RandomnessMatch]) -> Self {
        Self {
            recorded: Vec::from(recorded),
            fallback: TestRand,
        }
    }

    pub fn matches(&self, requested: RandomnessRule) -> Option<[u8; 32]> {
        for other in &self.recorded {
            if other.on == requested {
                let mut randomness = [0u8; 32];
                randomness.copy_from_slice(&other.ret);
                return Some(randomness);
            }
        }
        None
    }
}

impl Rand for ReplayingRand {
    // TODO: Check if this is going to be correct for when we integrate v5 Actors test vectors
    fn get_chain_randomness(
        &self,
        dst: DomainSeparationTag,
        epoch: ChainEpoch,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
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
    ) -> anyhow::Result<[u8; 32]> {
        let rule = RandomnessRule {
            kind: RandomnessKind::Chain,
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
}
