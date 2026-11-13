// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::address::Address;
use ahash::HashMap;
use parking_lot::Mutex;
use std::hash::Hash;
use std::sync::Arc;
use tokio::sync::OwnedMutexGuard;

/// Per-key async lock; same key serializes, other keys run in parallel.
/// Idle map entries are pruned on acquire when no other `Arc` remains.
pub(crate) struct KeyedLock<K> {
    inner: Mutex<HashMap<K, Arc<tokio::sync::Mutex<()>>>>,
}

impl<K> Default for KeyedLock<K> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(HashMap::default()),
        }
    }
}

impl<K: Eq + Hash> KeyedLock<K> {
    pub(crate) async fn take_lock(&self, key: K) -> OwnedMutexGuard<()> {
        let mutex = {
            let mut map = self.inner.lock();
            map.retain(|_, v| Arc::strong_count(v) > 1);
            map.entry(key)
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        mutex.lock_owned().await
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner.lock().len()
    }
}

/// Per-sender lock for `MpoolPushMessage`.
#[derive(Default)]
pub struct MpoolLocker {
    inner: KeyedLock<Address>,
}

impl MpoolLocker {
    pub async fn take_lock(&self, addr: Address) -> OwnedMutexGuard<()> {
        self.inner.take_lock(addr).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;
    use tokio::time::{Duration, timeout};

    #[tokio::test]
    async fn test_take_lock_serializes_same_key() {
        let locker = Arc::new(KeyedLock::default());
        let key = Address::new_id(1);

        let (first_acquired_tx, first_acquired_rx) = oneshot::channel();
        let (release_first_tx, release_first_rx) = oneshot::channel();
        let (second_acquired_tx, second_acquired_rx) = oneshot::channel();

        let locker2 = locker.clone();
        let t1 = tokio::spawn(async move {
            let _guard = locker2.take_lock(key).await;
            let _ = first_acquired_tx.send(());
            let _ = release_first_rx.await;
        });

        // Ensure task 1 is holding the lock before starting task 2.
        first_acquired_rx.await.unwrap();

        let locker3 = locker.clone();
        let t2 = tokio::spawn(async move {
            let _guard = locker3.take_lock(key).await;
            let _ = second_acquired_tx.send(());
        });

        // Task 2 must remain blocked while task 1 holds the lock.
        assert!(
            timeout(Duration::from_millis(50), second_acquired_rx)
                .await
                .is_err(),
            "second task should not acquire the same key lock while first holds it"
        );

        let _ = release_first_tx.send(());
        t1.await.unwrap();
        t2.await.unwrap();
    }

    #[tokio::test]
    async fn test_take_lock_different_keys_in_parallel() {
        let locker = Arc::new(KeyedLock::default());
        let key_a = Address::new_id(1);
        let key_b = Address::new_id(2);

        let (a_acquired_tx, a_acquired_rx) = oneshot::channel();
        let (release_a_tx, release_a_rx) = oneshot::channel();
        let (b_acquired_tx, b_acquired_rx) = oneshot::channel();

        let locker_a = locker.clone();
        let t_a = tokio::spawn(async move {
            let _guard = locker_a.take_lock(key_a).await;
            let _ = a_acquired_tx.send(());
            let _ = release_a_rx.await;
        });

        a_acquired_rx.await.unwrap();

        let locker_b = locker.clone();
        let t_b = tokio::spawn(async move {
            let _guard = locker_b.take_lock(key_b).await;
            let _ = b_acquired_tx.send(());
        });

        // Key B must acquire while key A is still held.
        timeout(Duration::from_millis(50), b_acquired_rx)
            .await
            .expect("timeout waiting for different-key lock")
            .expect("task holding key B dropped before sending");

        let _ = release_a_tx.send(());
        t_a.await.unwrap();
        t_b.await.unwrap();
    }

    #[tokio::test]
    async fn test_take_lock_prunes_idle_entries() {
        let locker = KeyedLock::default();
        let key_a = Address::new_id(1);
        let key_b = Address::new_id(2);

        {
            let _guard = locker.take_lock(key_a).await;
            assert_eq!(locker.len(), 1);
        }
        // Locking a different key triggers retain, which prunes key_a as guard is now dropped.
        let _guard_b = locker.take_lock(key_b).await;
        assert_eq!(
            locker.len(),
            1,
            "idle entry for key_a should have been pruned"
        );
    }
}
