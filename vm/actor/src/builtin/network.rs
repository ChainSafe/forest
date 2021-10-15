// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub use clock::EPOCH_DURATION_SECONDS;
pub use fil_types::BLOCKS_PER_EPOCH as EXPECTED_LEADERS_PER_EPOCH;
use num_bigint::BigInt;

pub const SECONDS_IN_HOUR: i64 = 3600;
pub const SECONDS_IN_DAY: i64 = 86400;
pub const SECONDS_IN_YEAR: i64 = 31556925;
pub const EPOCHS_IN_HOUR: i64 = SECONDS_IN_HOUR / EPOCH_DURATION_SECONDS;
pub const EPOCHS_IN_DAY: i64 = SECONDS_IN_DAY / EPOCH_DURATION_SECONDS;
pub const EPOCHS_IN_YEAR: i64 = SECONDS_IN_YEAR / EPOCH_DURATION_SECONDS;

/// Precision used for making QA power calculations
pub const SECTOR_QUALITY_PRECISION: i64 = 20;

lazy_static! {
    /// Quality multiplier for committed capacity (no deals) in a sector
    pub static ref QUALITY_BASE_MULTIPLIER: BigInt = BigInt::from(10);

    /// Quality multiplier for unverified deals in a sector
    pub static ref DEAL_WEIGHT_MULTIPLIER: BigInt = BigInt::from(10);

    /// Quality multiplier for verified deals in a sector
    pub static ref VERIFIED_DEAL_WEIGHT_MULTIPLIER: BigInt = BigInt::from(100);
}
