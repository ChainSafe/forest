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
    const TOTAL_FILECOIN: i64;

    /// Available rewards for mining.
    const MINING_REWARD_TOTAL: i64;

    /// Initial reward actor balance. This function is only called in genesis setting up state.
    fn initial_reward_balance() -> BigInt {
        BigInt::from(Self::MINING_REWARD_TOTAL) * Self::TOTAL_FILECOIN
    }

    /// Convert integer value of tokens into BigInt based on the token precision.
    fn from_fil(i: i64) -> BigInt {
        BigInt::from(i) * FILECOIN_PRECISION
    }
}

// Not yet finalized
pub struct DevnetParams;
impl NetworkParams for DevnetParams {
    const TOTAL_FILECOIN: i64 = 2_000_000_000;
    const MINING_REWARD_TOTAL: i64 = 1_400_000_000;
}

/// Ratio of integer values to token value.
pub const FILECOIN_PRECISION: i64 = 1_000_000_000_000_000_000;
