// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub use fvm_shared3::ALLOWABLE_CLOCK_DRIFT;
pub use fvm_shared3::BLOCKS_PER_EPOCH;
pub use fvm_shared3::clock::EPOCH_DURATION_SECONDS;

pub const SECONDS_IN_DAY: i64 = 86400;
pub const EPOCHS_IN_DAY: i64 = SECONDS_IN_DAY / EPOCH_DURATION_SECONDS;

pub type ChainEpoch = i64;
