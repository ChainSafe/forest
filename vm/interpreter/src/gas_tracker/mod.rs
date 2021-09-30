// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod gas_charge;
mod price_list;

pub use self::gas_charge::GasCharge;
pub use self::price_list::{price_list_by_epoch, PriceList};
use vm::{actor_error, ActorError, ExitCode};

pub(crate) struct GasTracker {
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

    /// Safely consumes gas and returns an out of gas error if there is not sufficient
    /// enough gas remaining for charge.
    pub fn charge_gas(&mut self, charge: GasCharge) -> Result<(), ActorError> {
        let to_use = charge.total();
        let used = self.gas_used + to_use;
        if used > self.gas_available {
            self.gas_used = self.gas_available;
            Err(actor_error!(SysErrOutOfGas;
                    "not enough gas (used={}) (available={})",
               used, self.gas_available
            ))
        } else {
            self.gas_used += to_use;
            Ok(())
        }
    }

    /// Getter for gas available.
    pub fn gas_available(&self) -> i64 {
        self.gas_available
    }

    /// Getter for gas used.
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
        t.charge_gas(GasCharge::new("", 5, 0)).unwrap();
        assert_eq!(t.gas_used(), 15);
        t.charge_gas(GasCharge::new("", 5, 0)).unwrap();
        assert_eq!(t.gas_used(), 20);
        assert!(t.charge_gas(GasCharge::new("", 1, 0)).is_err())
    }
}
