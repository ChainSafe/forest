// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod build_version;
pub mod deadlines;
mod piece;
mod randomness;
pub mod sector;
mod state;
mod version;

#[cfg(feature = "json")]
pub mod genesis;

#[cfg(feature = "proofs")]
pub mod verifier;

pub use self::piece::*;
pub use self::randomness::*;
pub use self::sector::*;
pub use self::state::*;
pub use self::version::*;

use address::Address;
use clock::ChainEpoch;
use num_bigint::BigInt;

#[macro_use]
extern crate lazy_static;

lazy_static! {
    /// Total Filecoin available to the network.
    pub static ref TOTAL_FILECOIN: BigInt = BigInt::from(TOTAL_FILECOIN_BASE) * FILECOIN_PRECISION;
    /// Amount of total Filecoin reserved in a static ID address.
    pub static ref FIL_RESERVED: BigInt = BigInt::from(300_000_000) * FILECOIN_PRECISION;

    /// Zero address used to avoid allowing it to be used for verification.
    /// This is intentionally disallowed because it is an edge case with Filecoin's BLS
    /// signature verification.
    pub static ref ZERO_ADDRESS: Address = "f3yaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaby2smx7a".parse().unwrap();
}

/// Identifier for Actors, includes builtin and initialized actors
pub type ActorID = u64;

/// Default bit width for the hamt in the filecoin protocol.
pub const HAMT_BIT_WIDTH: u32 = 5;
/// Total gas limit allowed per block. This is shared across networks.
pub const BLOCK_GAS_LIMIT: i64 = 10_000_000_000;
/// Total Filecoin supply.
pub const TOTAL_FILECOIN_BASE: i64 = 2_000_000_000;

// Epochs
/// Lookback height for retrieving ticket randomness.
pub const TICKET_RANDOMNESS_LOOKBACK: ChainEpoch = 1;
/// Epochs to look back for verifying PoSt proofs.
pub const WINNING_POST_SECTOR_SET_LOOKBACK: ChainEpoch = 10;

/// The expected number of block producers in each epoch.
pub const BLOCKS_PER_EPOCH: u64 = 5;

/// Ratio of integer values to token value.
pub const FILECOIN_PRECISION: i64 = 1_000_000_000_000_000_000;

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

/// Params for the network. This is now continued on into mainnet and is static across networks.
// * This can be removed in the future if the new testnet is configred at build time
// * but the reason to keep as is, is for an easier transition to runtime configuration.
pub struct DefaultNetworkParams;
impl NetworkParams for DefaultNetworkParams {
    const TOTAL_FILECOIN: i64 = TOTAL_FILECOIN_BASE;
    const MINING_REWARD_TOTAL: i64 = 1_400_000_000;
}
