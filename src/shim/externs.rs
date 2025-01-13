// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm2::externs::Rand as Rand_v2;
use fvm3::externs::Rand as Rand_v3;
use fvm4::externs::Rand as Rand_v4;
use fvm_shared2::clock::ChainEpoch as ChainEpoch_v2;
use fvm_shared3::clock::ChainEpoch as ChainEpoch_v3;
use fvm_shared4::clock::ChainEpoch as ChainEpoch_v4;

#[derive(Clone, Debug)]
pub struct RandWrapper<T> {
    pub chain_rand: T,
}

pub trait Rand {
    fn get_chain_randomness(&self, round: ChainEpoch_v2) -> anyhow::Result<[u8; 32]>;
    fn get_beacon_randomness(&self, round: ChainEpoch_v2) -> anyhow::Result<[u8; 32]>;
}

impl Rand for Box<dyn Rand> {
    fn get_chain_randomness(&self, round: ChainEpoch_v2) -> anyhow::Result<[u8; 32]> {
        self.as_ref().get_chain_randomness(round)
    }
    fn get_beacon_randomness(&self, round: ChainEpoch_v2) -> anyhow::Result<[u8; 32]> {
        self.as_ref().get_beacon_randomness(round)
    }
}

impl<T: Rand> Rand_v2 for RandWrapper<T> {
    fn get_chain_randomness(&self, round: ChainEpoch_v2) -> anyhow::Result<[u8; 32]> {
        self.chain_rand.get_chain_randomness(round)
    }

    fn get_beacon_randomness(&self, round: ChainEpoch_v2) -> anyhow::Result<[u8; 32]> {
        self.chain_rand.get_beacon_randomness(round)
    }
}

impl<T: Rand> Rand_v3 for RandWrapper<T> {
    fn get_chain_randomness(&self, round: ChainEpoch_v3) -> anyhow::Result<[u8; 32]> {
        self.chain_rand.get_chain_randomness(round)
    }

    fn get_beacon_randomness(&self, round: ChainEpoch_v3) -> anyhow::Result<[u8; 32]> {
        self.chain_rand.get_beacon_randomness(round)
    }
}

impl<T: Rand> Rand_v4 for RandWrapper<T> {
    fn get_chain_randomness(&self, round: ChainEpoch_v4) -> anyhow::Result<[u8; 32]> {
        self.chain_rand.get_chain_randomness(round)
    }

    fn get_beacon_randomness(&self, round: ChainEpoch_v4) -> anyhow::Result<[u8; 32]> {
        self.chain_rand.get_beacon_randomness(round)
    }
}

impl<T> From<T> for RandWrapper<T> {
    fn from(chain_rand: T) -> Self {
        RandWrapper { chain_rand }
    }
}
