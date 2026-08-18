// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use parking_lot::Mutex;
use std::sync::Arc;

/// A non-blocking fan-out publisher.
///
/// Each subscriber gets its own [`flume`] queue and cloning shares the subscriber
/// registry. [`Self::subscribe`] gives a subscriber an unbounded, lossless queue;
/// [`Self::subscribe_bounded`] gives a bounded queue that drops new events for that
/// subscriber alone once it is full (use it for best-effort consumers that must not be
/// able to grow memory without bound, e.g. ones fed by untrusted clients). Either way the
/// producer never blocks and a slow subscriber never stalls the others.
pub struct Publisher<T>(Arc<Mutex<Vec<flume::Sender<T>>>>);

impl<T> Clone for Publisher<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Default for Publisher<T> {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }
}

impl<T: Clone> Publisher<T> {
    /// Registers a new subscriber with an unbounded, lossless queue and returns its receiver.
    pub fn subscribe(&self) -> flume::Receiver<T> {
        let (tx, rx) = flume::unbounded();
        self.0.lock().push(tx);
        rx
    }

    /// Registers a new subscriber with a bounded queue of capacity `cap`. When the subscriber
    /// falls `cap` events behind, the newest events are dropped for it alone (it keeps the
    /// oldest `cap`; the producer and other subscribers are unaffected).
    pub fn subscribe_bounded(&self, cap: usize) -> flume::Receiver<T> {
        let (tx, rx) = flume::bounded(cap);
        self.0.lock().push(tx);
        rx
    }

    /// Delivers `msg` to every subscriber. Never blocks: for a bounded subscriber that is
    /// full the event is dropped for that subscriber; a subscriber whose receiver is gone
    /// is pruned.
    pub fn publish(&self, msg: T) {
        self.0.lock().retain(|tx| {
            !matches!(
                tx.try_send(msg.clone()),
                Err(flume::TrySendError::Disconnected(_))
            )
        });
    }

    /// Cheap check for whether any subscriber is registered. Does not prune, so it may
    /// briefly report `true` after the last receiver is gone (until the next [`Self::publish`]
    /// prunes it), but never reports `false` while a live subscriber exists.
    pub fn has_subscribers(&self) -> bool {
        !self.0.lock().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Itertools as _;

    #[test]
    fn publisher_is_lossless_under_lag() {
        let publisher = Publisher::default();
        let rx1 = publisher.subscribe();
        let rx2 = publisher.subscribe();

        // Far more than any bounded channel would hold; nothing is drained meanwhile.
        const N: u32 = 10_000;
        for i in 0..N {
            publisher.publish(i);
        }

        for rx in [&rx1, &rx2] {
            for expected in 0..N {
                assert_eq!(rx.recv().unwrap(), expected);
            }
            assert!(rx.try_recv().is_err());
        }
    }

    #[test]
    fn publisher_prunes_dropped_subscribers() {
        let publisher = Publisher::<u32>::default();
        let rx_live = publisher.subscribe();
        let rx_dead = publisher.subscribe();
        assert!(publisher.has_subscribers());

        drop(rx_dead);
        // Publishing prunes the dead sender while still delivering to the live one.
        publisher.publish(7);
        assert!(publisher.has_subscribers());
        assert_eq!(rx_live.recv().unwrap(), 7);

        drop(rx_live);
        publisher.publish(8);
        assert!(!publisher.has_subscribers());
    }

    #[test]
    fn publisher_bounded_subscriber_drops_without_blocking_others() {
        let publisher = Publisher::default();
        let unbounded = publisher.subscribe();
        let bounded = publisher.subscribe_bounded(2);

        // Publishing well past the bound must not block and must not affect the unbounded sub.
        for i in 0..10 {
            publisher.publish(i);
        }

        // Bounded subscriber kept only up to its capacity; the excess was dropped for it alone.
        let bounded_items = bounded.try_iter().collect_vec();
        assert_eq!(bounded_items, vec![0, 1]);

        // Unbounded subscriber still received everything, in order.
        let unbounded_items = unbounded.try_iter().collect_vec();
        assert_eq!(unbounded_items, (0..10).collect_vec());
    }
}
