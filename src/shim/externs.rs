// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::clock::ChainEpoch;

#[auto_impl::auto_impl(&, Arc, Box)]
pub trait Rand {
    fn get_chain_randomness(&self, round: ChainEpoch) -> anyhow::Result<[u8; 32]>;
    fn get_beacon_randomness(&self, round: ChainEpoch) -> anyhow::Result<[u8; 32]>;
}
