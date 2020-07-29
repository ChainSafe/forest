// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod price_list;

pub use self::price_list::{price_list_by_epoch, PriceList};
use vm::{actor_error, ActorError, ExitCode};

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
            Err(actor_error!(SysErrOutOfGas;
                "not enough gas (used={}) (available={})",
                self.gas_used + to_use,
                self.gas_available
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_gas_tracker() {
        let mut t = GasTracker::new(20, 10);
        t.charge_gas(5).unwrap();
        assert_eq!(t.gas_used(), 15);
        t.charge_gas(5).unwrap();
        assert_eq!(t.gas_used(), 20);
        assert!(t.charge_gas(1).is_err())
    }
}
