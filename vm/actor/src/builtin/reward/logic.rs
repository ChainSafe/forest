// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::math::PRECISION;
use fil_types::StoragePower;
use std::str::FromStr;

lazy_static! {
    /// Floor(e^(ln[1 + 200%] / epochsInYear) * 2^128
    /// Q.128 formatted number such that f(epoch) = baseExponent^epoch grows 200% in one
    /// year of epochs
    /// Calculation here: https://www.wolframalpha.com/input/?i=IntegerPart%5BExp%5BLog%5B1%2B200%25%5D%2F%28%28365+days%29%2F%2830+seconds%29%29%5D*2%5E128%5D
    pub static ref BASELINE_EXPONENT: StoragePower = StoragePower::from_str("340282722551251692435795578557183609728").unwrap();
    /// 1EiB
    pub static ref BASELINE_INITIAL_VALUE: StoragePower = StoragePower::from(1) << 60;
    /// 1EiB
    pub static ref INIT_BASELINE_POWER: StoragePower =
        ((BASELINE_INITIAL_VALUE.clone() << (2*PRECISION)) / &*BASELINE_EXPONENT) >> PRECISION;
}
