// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_types::StoragePower;
use num_traits::FromPrimitive;

/// Minimum power of an individual miner to meet the threshold for leader election.
pub const CONSENSUS_MINER_MIN_MINERS: i64 = 3;

/// Maximum number of prove commits a miner can submit in one epoch
///
/// We bound this to 200 to limit the number of prove partitions we may need to update in a
/// given epoch to 200.
///
/// To support onboarding 1EiB/year, we need to allow at least 32 prove commits per epoch.
pub const MAX_MINER_PROVE_COMMITS_PER_EPOCH: u64 = 3;

lazy_static! {
    /// Minimum power of an individual miner to meet the threshold for leader election.
    pub static ref CONSENSUS_MINER_MIN_POWER: StoragePower = StoragePower::from_i64(1 << 40).unwrap(); // placeholder
}
