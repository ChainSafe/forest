// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub use super::fvm_shared_latest::clock::EPOCH_DURATION_SECONDS;
pub use super::fvm_shared_latest::ALLOWABLE_CLOCK_DRIFT;
pub use super::fvm_shared_latest::BLOCKS_PER_EPOCH;

pub const SECONDS_IN_DAY: i64 = 86400;
pub const EPOCHS_IN_DAY: i64 = SECONDS_IN_DAY / EPOCH_DURATION_SECONDS;

pub type ChainEpoch = i64;
