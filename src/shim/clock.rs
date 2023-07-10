// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub use fvm_shared::ALLOWABLE_CLOCK_DRIFT;
pub use fvm_shared::BLOCKS_PER_EPOCH;

pub const SECONDS_IN_DAY: i64 = 86400;
pub const EPOCHS_IN_DAY: i64 = SECONDS_IN_DAY / EPOCH_DURATION_SECONDS;
pub const EPOCH_DURATION_SECONDS: i64 = 30;

pub type ChainEpoch = i64;

#[cfg(test)]
mod tests {
    #[test]
    fn fvm_shim_of_const_epoch_duration_seconds() {
        assert_eq!(
            super::EPOCH_DURATION_SECONDS,
            fvm_shared::clock::EPOCH_DURATION_SECONDS
        )
    }

    #[test]
    fn fvm3_shim_of_const_epoch_duration_seconds() {
        assert_eq!(
            super::EPOCH_DURATION_SECONDS,
            fvm_shared3::clock::EPOCH_DURATION_SECONDS
        )
    }
}
