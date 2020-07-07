// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub const EPOCH_DURATION_SECONDS: i64 = 25;
pub const SECONDS_IN_HOUR: i64 = 3600;
pub const SECONDS_IN_DAY: i64 = 86400;
pub const SECONDS_IN_YEAR: i64 = 31556925;
pub const EPOCHS_IN_HOUR: i64 = SECONDS_IN_HOUR / EPOCH_DURATION_SECONDS;
pub const EPOCHS_IN_DAY: i64 = SECONDS_IN_DAY / EPOCH_DURATION_SECONDS;
pub const EPOCHS_IN_YEAR: i64 = SECONDS_IN_YEAR / EPOCH_DURATION_SECONDS;

/// The expected number of block producers in each epoch.
pub const EXPECTED_LEADERS_PER_EPOCH: u64 = 5;
