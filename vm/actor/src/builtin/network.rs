// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub use clock::EPOCH_DURATION_SECONDS;

pub const SECONDS_IN_HOUR: i64 = 3600;
pub const SECONDS_IN_DAY: i64 = 86400;
pub const SECONDS_IN_YEAR: i64 = 31556925;
pub const EPOCHS_IN_HOUR: i64 = SECONDS_IN_HOUR / EPOCH_DURATION_SECONDS;
pub const EPOCHS_IN_DAY: i64 = SECONDS_IN_DAY / EPOCH_DURATION_SECONDS;
pub const EPOCHS_IN_YEAR: i64 = SECONDS_IN_YEAR / EPOCH_DURATION_SECONDS;

/// The expected number of block producers in each epoch.
pub const EXPECTED_LEADERS_PER_EPOCH: u64 = 5;

/// Quality multiplier for committed capacity (no deals) in a sector
pub const QUALITY_BASE_MULTIPLIER: i64 = 10;

/// Quality multiplier for unverified deals in a sector
pub const DEAL_WEIGHT_MULTIPLIER: i64 = 10;

/// Quality multiplier for verified deals in a sector
pub const VERIFIED_DEAL_WEIGHT_MULTIPLIER: i64 = 100;

/// Precision used for making QA power calculations
pub const SECTOR_QUALITY_PRECISION: i64 = 20;
