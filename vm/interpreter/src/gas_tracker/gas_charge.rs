// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm::gas::Gas;

/// Single gas charge in the VM. Contains information about what gas was for, as well
/// as the amount of gas needed for computation and storage respectively.
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
        self.compute_gas + self.storage_gas
    }
}

impl From<GasCharge> for fvm::gas::GasCharge<'_> {
    fn from(charge: GasCharge) -> Self {
        Self {
            name: charge.name,
            compute_gas: Gas::new(charge.compute_gas),
            storage_gas: Gas::new(charge.storage_gas),
        }
    }
}
