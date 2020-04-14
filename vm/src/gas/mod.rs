// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod price_list;

pub use self::price_list::{price_list_by_epoch, PriceList};
use crate::{ActorError, ExitCode};

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

    /// Safely consumes gas
    pub fn charge_gas(&mut self, to_use: i64) -> Result<(), ActorError> {
        if self.gas_used + to_use > self.gas_available {
            self.gas_used = self.gas_available;
            Err(ActorError::new(
                ExitCode::SysErrOutOfGas,
                format!(
                    "not enough gas (used={}) (available={})",
                    self.gas_used + to_use,
                    self.gas_available
                ),
            ))
        } else {
            self.gas_used += to_use;
            Ok(())
        }
    }

    /// Getter for gas available
    pub fn gas_available(&self) -> i64 {
        self.gas_available
    }

    /// Getter for gas used
    pub fn gas_used(&self) -> i64 {
        self.gas_used
    }
}
