// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

const EPOCH_DURATION_SECONDS: u64 = 25;

const SECONDS_IN_YEAR: u64 = 31_556_925;

pub const EPOCHS_IN_YEAR: u64 = SECONDS_IN_YEAR / EPOCH_DURATION_SECONDS;
