// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::address::Address;
use ahash::HashMap;
use std::sync::{Arc, Mutex};
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
            let mut map = self.inner.lock().expect("MpoolLocker poisoned");
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
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn test_take_lock_serializes_same_address() {
        let locker = Arc::new(MpoolLocker::new());
        let addr = Address::new_id(1);

        let first_entered = Arc::new(AtomicBool::new(false));
        let first_released = Arc::new(AtomicBool::new(false));
        let second_saw_first = Arc::new(AtomicBool::new(false));

        let locker2 = locker.clone();
        let entered = first_entered.clone();
        let released = first_released.clone();
        let t1 = tokio::spawn(async move {
            let _guard = locker2.take_lock(addr).await;
            entered.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(100)).await;
            released.store(true, Ordering::SeqCst);
        });

        // Give task 1 time to acquire the lock
        tokio::time::sleep(Duration::from_millis(20)).await;

        let locker3 = locker.clone();
        let saw = second_saw_first.clone();
        let rel = first_released.clone();
        let t2 = tokio::spawn(async move {
            let _guard = locker3.take_lock(addr).await;
            saw.store(rel.load(Ordering::SeqCst), Ordering::SeqCst);
        });

        t1.await.unwrap();
        t2.await.unwrap();

        assert!(
            first_entered.load(Ordering::SeqCst),
            "first task should have entered"
        );
        assert!(
            second_saw_first.load(Ordering::SeqCst),
            "second task should only proceed after first released"
        );
    }

    #[tokio::test]
    async fn test_take_lock_allows_different_addresses() {
        let locker = Arc::new(MpoolLocker::new());
        let addr_a = Address::new_id(1);
        let addr_b = Address::new_id(2);

        let both_held = Arc::new(AtomicBool::new(false));

        let locker2 = locker.clone();
        let flag = both_held.clone();
        let t1 = tokio::spawn(async move {
            let _guard = locker2.take_lock(addr_a).await;
            tokio::time::sleep(Duration::from_millis(100)).await;
            flag.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(100)).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;

        let locker3 = locker.clone();
        let flag2 = both_held.clone();
        let t2 = tokio::spawn(async move {
            let _guard = locker3.take_lock(addr_b).await;
            assert!(
                !flag2.load(Ordering::SeqCst),
                "second task should acquire lock before first finishes sleeping"
            );
        });

        t1.await.unwrap();
        t2.await.unwrap();
    }
}
