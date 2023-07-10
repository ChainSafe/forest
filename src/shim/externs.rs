// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm2::externs::Rand as Rand_v2;
use fvm3::externs::Rand as Rand_v3;
use fvm_shared2::clock::ChainEpoch as ChainEpoch_v2;
use fvm_shared3::clock::ChainEpoch as ChainEpoch_v3;

#[derive(Clone, Debug)]
pub struct RandWrapper<T> {
    pub chain_rand: T,
}

pub trait Rand {
    fn get_chain_randomness(
        &self,
        pers: i64,
        round: ChainEpoch_v2,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]>;
    fn get_beacon_randomness(
        &self,
        pers: i64,
        round: ChainEpoch_v2,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]>;
}

impl<T: Rand> Rand_v2 for RandWrapper<T> {
    fn get_chain_randomness(
        &self,
        pers: i64,
        round: ChainEpoch_v2,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        self.chain_rand.get_chain_randomness(pers, round, entropy)
    }

    fn get_beacon_randomness(
        &self,
        pers: i64,
        round: ChainEpoch_v2,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        self.chain_rand.get_beacon_randomness(pers, round, entropy)
    }
}

impl<T: Rand> Rand_v3 for RandWrapper<T> {
    fn get_chain_randomness(
        &self,
        pers: i64,
        round: ChainEpoch_v3,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        self.chain_rand.get_chain_randomness(pers, round, entropy)
    }

    fn get_beacon_randomness(
        &self,
        pers: i64,
        round: ChainEpoch_v3,
        entropy: &[u8],
    ) -> anyhow::Result<[u8; 32]> {
        self.chain_rand.get_beacon_randomness(pers, round, entropy)
    }
}

impl<T> From<T> for RandWrapper<T> {
    fn from(chain_rand: T) -> Self {
        RandWrapper { chain_rand }
    }
}
