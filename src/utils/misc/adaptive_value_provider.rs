// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use parking_lot::RwLock;

pub trait AdaptiveValueProvider<T: num::PrimInt> {
    fn get(&self) -> T;

    fn adapt_on_success(&self, record: T) -> bool;

    fn adapt_on_failure(&self);
}

pub struct ExponentialAdaptiveValueProvider<T: num::PrimInt> {
    value: RwLock<T>,
    min: T,
    max: T,
    increase_on_success: bool,
}

impl<T: num::PrimInt> ExponentialAdaptiveValueProvider<T> {
    pub fn new(value: T, min: T, max: T, increase_on_success: bool) -> Self {
        Self {
            value: RwLock::new(value),
            min,
            max,
            increase_on_success,
        }
    }

    fn increase(&self, record: Option<T>) -> bool {
        let current = self.get();
        if current == self.max {
            return false;
        }
        let new_value = current.shl(1).min(self.max);
        if let Some(record) = record {
            if record < new_value {
                return false;
            }
        }
        *self.value.write() = new_value;
        true
    }

    fn decrease(&self, record: Option<T>) -> bool {
        let current = self.get();
        if current == self.min {
            return false;
        }
        let new_value = current.shr(1).max(self.min);
        if let Some(record) = record {
            if record > new_value {
                return false;
            }
        }
        *self.value.write() = new_value;
        true
    }
}

impl<T: num::PrimInt> AdaptiveValueProvider<T> for ExponentialAdaptiveValueProvider<T> {
    fn get(&self) -> T {
        *self.value.read()
    }

    fn adapt_on_success(&self, record: T) -> bool {
        if self.increase_on_success {
            self.increase(Some(record))
        } else {
            self.decrease(Some(record))
        }
    }

    fn adapt_on_failure(&self) {
        if !self.increase_on_success {
            self.increase(None);
        } else {
            self.decrease(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_adaptive_value_provider_behaviour() {
        let p = ExponentialAdaptiveValueProvider::new(8, 2, 60, false);
        assert_eq!(p.get(), 8);
        assert!(!p.adapt_on_success(5));
        assert_eq!(p.get(), 8);
        assert!(p.adapt_on_success(4));
        assert_eq!(p.get(), 4);
        assert!(!p.adapt_on_success(3));
        assert_eq!(p.get(), 4);
        assert!(p.adapt_on_success(2));
        assert_eq!(p.get(), 2);
        assert!(!p.adapt_on_success(1));
        assert_eq!(p.get(), 2);
        assert!(!p.adapt_on_success(1));
        assert_eq!(p.get(), 2);
        p.adapt_on_failure();
        assert_eq!(p.get(), 4);
        p.adapt_on_failure();
        assert_eq!(p.get(), 8);
        p.adapt_on_failure();
        assert_eq!(p.get(), 16);
        p.adapt_on_failure();
        assert_eq!(p.get(), 32);
        p.adapt_on_failure();
        assert_eq!(p.get(), 60);
        p.adapt_on_failure();
        assert_eq!(p.get(), 60);
    }
}
