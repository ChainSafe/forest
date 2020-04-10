// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod price_list;

pub use self::price_list::{price_list_by_epoch, PriceList};

pub struct GasTracker {
    gas_available: i64,
    gas_used: i64,
}

impl GasTracker {
    pub fn new(gas_available: i64, gas_used: i64) -> Self {
        Self {
            gas_available,
            gas_used,
        }
    }
}
