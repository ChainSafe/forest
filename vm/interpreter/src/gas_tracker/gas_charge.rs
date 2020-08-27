// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

const GAS_STORAGE_MULTI: i64 = 1000;
const GAS_COMPUTE_MULTI: i64 = 1;

pub struct GasCharge {
    pub name: &'static str,
    pub compute_gas: i64,
    pub storage_gas: i64,
}

impl GasCharge {
    pub fn new(name: &'static str, compute_gas: i64, storage_gas: i64) -> Self {
        Self {
            name,
            compute_gas,
            storage_gas,
        }
    }

    /// Calculates total gas charge based on compute and storage multipliers.
    pub fn total(&self) -> i64 {
        self.compute_gas * GAS_COMPUTE_MULTI + self.storage_gas * GAS_STORAGE_MULTI
    }
}
