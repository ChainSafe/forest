// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

/// Duration of each tipset epoch.
pub const EPOCH_DURATION_SECONDS: i64 = 30;

pub const SECONDS_IN_DAY: i64 = 86400;
pub const EPOCHS_IN_DAY: i64 = SECONDS_IN_DAY / EPOCH_DURATION_SECONDS;
