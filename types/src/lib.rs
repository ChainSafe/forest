// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod genesis;
mod piece;
pub mod sector;

pub use self::piece::*;
pub use self::sector::*;

use clock::{ChainEpoch, EPOCH_DURATION_SECONDS};
use num_bigint::BigInt;

/// Default bit width for the hamt in the filecoin protocol.
pub const HAMT_BIT_WIDTH: u32 = 5;
pub const BLOCK_GAS_LIMIT: i64 = 10_000_000_000;
pub const TOTAL_FILECOIN: i64 = 2_000_000_000;

// Epochs
pub const TICKET_RANDOMNESS_LOOKBACK: ChainEpoch = 1;
pub const WINNING_POST_SECTOR_SET_LOOKBACK: ChainEpoch = 10;

/// The expected number of block producers in each epoch.
pub const BLOCKS_PER_EPOCH: u64 = 5;

/// Ratio of integer values to token value.
pub const FILECOIN_PRECISION: i64 = 1_000_000_000_000_000_000;

/// Block delay, or epoch duration, to be used in blockchain system.
pub const BLOCK_DELAY_SECS: u64 = EPOCH_DURATION_SECONDS as u64;

/// Allowable clock drift in validations.
pub const ALLOWABLE_CLOCK_DRIFT: u64 = 1;

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

// Devnet Parameters, not yet finalized
pub struct DevnetParams;
impl NetworkParams for DevnetParams {
    const TOTAL_FILECOIN: i64 = TOTAL_FILECOIN;
    const MINING_REWARD_TOTAL: i64 = 1_400_000_000;
}

/// Space-race specific fork
pub const UPGRADE_SMOKE_HEIGHT: i64 = 51000;
