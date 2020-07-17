// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod piece;
pub mod sector;

pub use self::piece::*;
pub use self::sector::*;

use num_bigint::BigInt;

/// Config trait which handles different network configurations.
pub trait NetworkParams {
    /// Total filecoin available to network.
    const TOTAL_FILECOIN: u64;

    /// Available rewards for mining.
    const MINING_REWARD_TOTAL: u64;

    /// Initial reward actor balance. This function is only called in genesis setting up state.
    fn initial_reward_balance() -> BigInt {
        BigInt::from(Self::MINING_REWARD_TOTAL) * Self::TOTAL_FILECOIN
    }
}

// Not yet finalized
pub struct DevnetParams;
impl NetworkParams for DevnetParams {
    const TOTAL_FILECOIN: u64 = 2_000_000_000;
    const MINING_REWARD_TOTAL: u64 = 1_400_000_000;
}

pub const FILECOIN_PRECISION: u64 = 1_000_000_000_000_000_000;
