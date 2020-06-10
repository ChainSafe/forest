// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub const EPOCH_DURATION_SECONDS: u64 = 25;
pub const SECONDS_IN_HOUR: u64 = 3600;
pub const SECONDS_IN_DAY: u64 = 86400;
pub const SECONDS_IN_YEAR: u64 = 31556925;
pub const EPOCHS_IN_HOUR: u64 = SECONDS_IN_HOUR / EPOCH_DURATION_SECONDS;
pub const EPOCHS_IN_DAY: u64 = SECONDS_IN_DAY / EPOCH_DURATION_SECONDS;
pub const EPOCHS_IN_YEAR: u64 = SECONDS_IN_YEAR / EPOCH_DURATION_SECONDS;

/// The expected number of block producers in each epoch.
pub const EXPECTED_LEADERS_PER_EPOCH: u64 = 5;
