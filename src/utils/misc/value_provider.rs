// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    fmt::Display,
    sync::atomic::{AtomicUsize, Ordering},
};

const ORDERING: Ordering = Ordering::Relaxed;

pub struct AdaptiveValueProviderConfig {
    /// Value name for tracing
    pub name: String,
    /// Value downgrading threshold after provided consecutive failure(s) (Exclusive)
    pub downgrade_threshod: usize,
    /// Value upgrading threshold after provided consecutive success(es) (Exclusive)
    pub upgrade_threshold: usize,
    /// Whether to enable tracing on value upgrade and downgrade, tracing level is `INFO`
    pub tracing: bool,
}

pub struct AdaptiveValueProvider<T> {
    values: Vec<T>,
    config: AdaptiveValueProviderConfig,
    current_index: AtomicUsize,
    consective_success_counter: AtomicUsize,
    consective_failure_counter: AtomicUsize,
}

impl<T: Display> AdaptiveValueProvider<T> {
    /// Creates an adaptive value provider with a vector of value options (downgraded values on the right)
    pub fn new(values: Vec<T>, config: AdaptiveValueProviderConfig) -> Self {
        if values.is_empty() {
            panic!("Input values cannot be empty");
        }

        Self {
            values,
            config,
            current_index: Default::default(),
            consective_success_counter: Default::default(),
            consective_failure_counter: Default::default(),
        }
    }

    /// Gets the current value
    pub fn value(&self) -> &T {
        &self.values[self.current_index.load(ORDERING)]
    }

    /// Tracks success, a value upgrade might be triggered
    pub fn track_success(&self) {
        let old = self.consective_success_counter.fetch_add(1, ORDERING);
        self.consective_failure_counter.store(0, ORDERING);
        if old + 1 > self.config.upgrade_threshold {
            self.current_index
                .fetch_update(ORDERING, ORDERING, |i| {
                    if i > 0 {
                        if self.config.tracing {
                            tracing::info!(
                                "[{}] value upgraded to {}",
                                self.config.name,
                                self.values[i - 1]
                            );
                        }
                        self.consective_success_counter.store(0, ORDERING);
                        Some(i - 1)
                    } else {
                        Some(i)
                    }
                })
                .expect("Infallible");
        }
    }

    /// Tracks success, a value downgrade might be triggered
    pub fn track_failure(&self) {
        let old = self.consective_failure_counter.fetch_add(1, ORDERING);
        self.consective_success_counter.store(0, ORDERING);
        if old + 1 > self.config.downgrade_threshod {
            let len = self.values.len();
            self.current_index
                .fetch_update(ORDERING, ORDERING, |i| {
                    if i + 1 < len {
                        if self.config.tracing {
                            tracing::info!(
                                "[{}] value downgraded to {}",
                                self.config.name,
                                self.values[i + 1]
                            );
                        }
                        self.consective_failure_counter.store(0, ORDERING);
                        Some(i + 1)
                    } else {
                        Some(i)
                    }
                })
                .expect("Infallible");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use quickcheck_macros::quickcheck;
    use rand::{rngs::OsRng, RngCore};
    use tracing_test::traced_test;

    #[test]
    #[traced_test]
    fn behaviour_test() {
        let value_provider = {
            let config = AdaptiveValueProviderConfig {
                name: "Tipset sync request window size".into(),
                tracing: true,
                upgrade_threshold: 2,
                downgrade_threshod: 2,
            };
            let mut size = 4;
            let mut values = vec![size];
            while size > 1 {
                size /= 2;
                values.push(size);
            }
            AdaptiveValueProvider::new(values, config)
        };

        assert_eq!(value_provider.value(), &4);

        value_provider.track_failure();
        assert_eq!(value_provider.value(), &4);
        value_provider.track_failure();
        assert_eq!(value_provider.value(), &4);

        // Downgrade on 3 consecutive failures
        value_provider.track_failure();
        assert_eq!(value_provider.value(), &2);

        value_provider.track_failure();
        assert_eq!(value_provider.value(), &2);
        value_provider.track_failure();
        assert_eq!(value_provider.value(), &2);

        // Reset downgrade counter
        value_provider.track_success();

        value_provider.track_failure();
        assert_eq!(value_provider.value(), &2);
        value_provider.track_failure();
        assert_eq!(value_provider.value(), &2);

        // Downgrade again on 3 consecutive failures
        value_provider.track_failure();
        assert_eq!(value_provider.value(), &1);

        // No more downgrades
        for _ in 0..100 {
            value_provider.track_failure();
            assert_eq!(value_provider.value(), &1);
        }

        value_provider.track_success();
        assert_eq!(value_provider.value(), &1);
        value_provider.track_success();
        assert_eq!(value_provider.value(), &1);

        // Upgrade on 3 consecutive successes
        value_provider.track_success();
        assert_eq!(value_provider.value(), &2);

        value_provider.track_success();
        assert_eq!(value_provider.value(), &2);
        value_provider.track_success();
        assert_eq!(value_provider.value(), &2);

        // Reset upgrade counter
        value_provider.track_failure();

        value_provider.track_success();
        assert_eq!(value_provider.value(), &2);
        value_provider.track_success();
        assert_eq!(value_provider.value(), &2);

        // Upgrade again on 3 consecutive successes
        value_provider.track_success();
        assert_eq!(value_provider.value(), &4);

        // No more upgrades
        for _ in 0..100 {
            value_provider.track_success();
            assert_eq!(value_provider.value(), &4);
        }
    }

    #[quickcheck]
    #[traced_test]
    fn overflow_tests(n: u16) {
        let value_provider = {
            let config = AdaptiveValueProviderConfig {
                name: "Tipset sync request window size".into(),
                tracing: false,
                upgrade_threshold: 2,
                downgrade_threshod: 2,
            };
            let mut size = 32;
            let mut values = vec![size];
            while size > 1 {
                size /= 2;
                values.push(size);
            }
            AdaptiveValueProvider::new(values, config)
        };

        for _ in 0..n {
            if OsRng.next_u32() % 2 == 0 {
                value_provider.track_success();
            } else {
                value_provider.track_failure();
            }
        }
    }
}
