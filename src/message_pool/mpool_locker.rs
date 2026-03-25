// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::address::Address;
use ahash::HashMap;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::OwnedMutexGuard;

/// Per-address async lock for serializing `MpoolPushMessage` RPC calls.
/// Concurrent pushes for the same sender block on each other, while
/// different senders proceed in parallel.
pub struct MpoolLocker {
    inner: Mutex<HashMap<Address, Arc<tokio::sync::Mutex<()>>>>,
}

impl MpoolLocker {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::default()),
        }
    }

    /// Acquire an async lock for the given address. The returned guard must be
    /// held for the duration of the nonce-assign + sign + push critical section.
    pub async fn take_lock(&self, addr: Address) -> OwnedMutexGuard<()> {
        let mutex = {
            let mut map = self.inner.lock();
            map.entry(addr)
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        mutex.lock_owned().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::{Barrier, oneshot};
    use tokio::time::{Duration, timeout};

    #[tokio::test]
    async fn test_take_lock_serializes_same_address() {
        let locker = Arc::new(MpoolLocker::new());
        let addr = Address::new_id(1);

        let (first_acquired_tx, first_acquired_rx) = oneshot::channel();
        let (release_first_tx, release_first_rx) = oneshot::channel();
        let (second_acquired_tx, second_acquired_rx) = oneshot::channel();

        let locker2 = locker.clone();
        let t1 = tokio::spawn(async move {
            let _guard = locker2.take_lock(addr).await;
            let _ = first_acquired_tx.send(());
            let _ = release_first_rx.await;
        });

        // Ensure task 1 is holding the lock before starting task 2.
        first_acquired_rx.await.unwrap();

        let locker3 = locker.clone();
        let t2 = tokio::spawn(async move {
            let _guard = locker3.take_lock(addr).await;
            let _ = second_acquired_tx.send(());
        });

        // Task 2 must remain blocked while task 1 holds the lock.
        assert!(
            timeout(Duration::from_millis(50), second_acquired_rx)
                .await
                .is_err(),
            "second task should not acquire the same address lock while first holds it"
        );

        let _ = release_first_tx.send(());
        t1.await.unwrap();
        t2.await.unwrap();
    }

    #[tokio::test]
    async fn test_take_lock_allows_different_addresses() {
        let locker = Arc::new(MpoolLocker::new());
        let addr_a = Address::new_id(1);
        let addr_b = Address::new_id(2);

        let acquired_barrier = Arc::new(Barrier::new(2));

        let locker2 = locker.clone();
        let barrier_a = acquired_barrier.clone();
        let t1 = tokio::spawn(async move {
            let _guard = locker2.take_lock(addr_a).await;
            barrier_a.wait().await;
        });

        let locker3 = locker.clone();
        let barrier_b = acquired_barrier.clone();
        let t2 = tokio::spawn(async move {
            let _guard = locker3.take_lock(addr_b).await;
            barrier_b.wait().await;
        });

        timeout(Duration::from_millis(200), async {
            t1.await.unwrap();
            t2.await.unwrap();
        })
        .await
        .expect("different address locks should be acquired in parallel");
    }
}
