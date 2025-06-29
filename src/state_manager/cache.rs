// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::blocks::TipsetKey;
use crate::shim::executor::Receipt;
use crate::state_manager::{DEFAULT_TIPSET_CACHE_SIZE, StateEvents};
use lru::LruCache;
use nonzero_ext::nonzero;
use parking_lot::Mutex as SyncMutex;
use std::future::Future;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

const DEFAULT_RECEIPT_AND_EVENT_CACHE_SIZE: NonZeroUsize = nonzero!(4096usize);

struct TipsetStateCacheInner<V> {
    values: LruCache<TipsetKey, V>,
    pending: Vec<(TipsetKey, Arc<TokioMutex<()>>)>,
}

impl<V: Clone> Default for TipsetStateCacheInner<V> {
    fn default() -> Self {
        Self {
            values: LruCache::new(DEFAULT_TIPSET_CACHE_SIZE),
            pending: Vec::with_capacity(8),
        }
    }
}

impl<V: Clone> TipsetStateCacheInner<V> {
    pub fn with_size(cache_size: NonZeroUsize) -> Self {
        Self {
            values: LruCache::new(cache_size),
            pending: Vec::with_capacity(8),
        }
    }
}

/// A generic cache that handles concurrent access and computation for tipset-related data.
pub(crate) struct TipsetStateCache<V> {
    cache: Arc<SyncMutex<TipsetStateCacheInner<V>>>,
}

impl<V: Clone> Default for TipsetStateCache<V> {
    fn default() -> Self {
        TipsetStateCache::with_size(DEFAULT_RECEIPT_AND_EVENT_CACHE_SIZE)
    }
}

enum CacheLookupStatus<V> {
    Exist(V),
    Empty(Arc<TokioMutex<()>>),
}

impl<V: Clone> TipsetStateCache<V> {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(SyncMutex::new(TipsetStateCacheInner::default())),
        }
    }

    pub fn with_size(cache_size: NonZeroUsize) -> Self {
        Self {
            cache: Arc::new(SyncMutex::new(TipsetStateCacheInner::with_size(cache_size))),
        }
    }

    fn with_inner<F, T>(&self, func: F) -> T
    where
        F: FnOnce(&mut TipsetStateCacheInner<V>) -> T,
    {
        let mut lock = self.cache.lock();
        func(&mut lock)
    }

    pub async fn get_or_else<F, Fut>(&self, key: &TipsetKey, compute: F) -> anyhow::Result<V>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = anyhow::Result<V>> + Send,
        V: Send + Sync + 'static,
    {
        let status = self.with_inner(|inner| match inner.values.get(key) {
            Some(v) => CacheLookupStatus::Exist(v.clone()),
            None => {
                let option = inner
                    .pending
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, mutex)| mutex);
                match option {
                    Some(mutex) => CacheLookupStatus::Empty(mutex.clone()),
                    None => {
                        let mutex = Arc::new(TokioMutex::new(()));
                        inner.pending.push((key.clone(), mutex.clone()));
                        CacheLookupStatus::Empty(mutex)
                    }
                }
            }
        });
        match status {
            CacheLookupStatus::Exist(x) => {
                crate::metrics::LRU_CACHE_HIT
                    .get_or_create(&crate::metrics::values::STATE_MANAGER_TIPSET)
                    .inc();
                Ok(x)
            }
            CacheLookupStatus::Empty(mtx) => {
                let _guard = mtx.lock().await;
                match self.get(key) {
                    Some(v) => {
                        // While locking someone else computed the pending task
                        crate::metrics::LRU_CACHE_HIT
                            .get_or_create(&crate::metrics::values::STATE_MANAGER_TIPSET)
                            .inc();

                        Ok(v)
                    }
                    None => {
                        // Entry does not have state computed yet, compute value and fill the cache
                        crate::metrics::LRU_CACHE_MISS
                            .get_or_create(&crate::metrics::values::STATE_MANAGER_TIPSET)
                            .inc();

                        let value = compute().await?;

                        // Write back to cache, release lock and return value
                        self.insert(key.clone(), value.clone());
                        Ok(value)
                    }
                }
            }
        }
    }

    pub fn get(&self, key: &TipsetKey) -> Option<V> {
        self.with_inner(|inner| inner.values.get(key).cloned())
    }

    pub fn insert(&self, key: TipsetKey, value: V) {
        self.with_inner(|inner| {
            inner.pending.retain(|(k, _)| k != &key);
            inner.values.put(key, value);
        });
    }
}

// Type alias for the compute function for receipts
type ComputeReceiptFn =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<Receipt>>> + Send>> + Send>;

// Type alias for the compute function for state events
type ComputeEventsFn =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = anyhow::Result<StateEvents>> + Send>> + Send>;

/// Defines the interface for caching and retrieving tipset-specific events and receipts.
pub trait TipsetReceiptEventCacheHandler: Send + Sync + 'static {
    fn insert_receipt(&self, key: &TipsetKey, receipt: Vec<Receipt>);
    fn insert_events(&self, key: &TipsetKey, events: StateEvents);
    #[allow(dead_code)]
    fn get_events(&self, key: &TipsetKey) -> Option<StateEvents>;
    #[allow(dead_code)]
    fn get_receipts(&self, key: &TipsetKey) -> Option<Vec<Receipt>>;
    fn get_receipt_or_else(
        &self,
        key: &TipsetKey,
        compute: ComputeReceiptFn,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<Receipt>>> + Send + '_>>;
    fn get_events_or_else(
        &self,
        key: &TipsetKey,
        compute: ComputeEventsFn,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<StateEvents>> + Send + '_>>;
}

/// Cache for tipset-related events and receipts.
pub struct EnabledTipsetDataCache {
    events_cache: TipsetStateCache<StateEvents>,
    receipt_cache: TipsetStateCache<Vec<Receipt>>,
}

impl EnabledTipsetDataCache {
    pub fn new() -> Self {
        Self {
            events_cache: TipsetStateCache::default(),
            receipt_cache: TipsetStateCache::default(),
        }
    }
}

impl TipsetReceiptEventCacheHandler for EnabledTipsetDataCache {
    fn insert_receipt(&self, key: &TipsetKey, receipts: Vec<Receipt>) {
        if !receipts.is_empty() {
            self.receipt_cache.insert(key.clone(), receipts);
        }
    }

    fn insert_events(&self, key: &TipsetKey, events_data: StateEvents) {
        if !events_data.events.is_empty() {
            self.events_cache.insert(key.clone(), events_data);
        }
    }

    fn get_events(&self, key: &TipsetKey) -> Option<StateEvents> {
        self.events_cache.get(key)
    }

    fn get_receipts(&self, key: &TipsetKey) -> Option<Vec<Receipt>> {
        self.receipt_cache.get(key)
    }

    fn get_receipt_or_else(
        &self,
        key: &TipsetKey,
        compute: ComputeReceiptFn,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<Receipt>>> + Send + '_>> {
        let key = key.clone();
        let receipt_cache = &self.receipt_cache;

        Box::pin(async move {
            receipt_cache
                .get_or_else(&key, || async move { compute().await })
                .await
        })
    }

    fn get_events_or_else(
        &self,
        key: &TipsetKey,
        compute: ComputeEventsFn,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<StateEvents>> + Send + '_>> {
        let key = key.clone();
        let events_cache = &self.events_cache;

        Box::pin(async move {
            events_cache
                .get_or_else(&key, || async move { compute().await })
                .await
        })
    }
}

/// Fake cache for tipset-related events and receipts.
pub struct DisabledTipsetDataCache;

impl DisabledTipsetDataCache {
    pub fn new() -> Self {
        Self {}
    }
}

impl TipsetReceiptEventCacheHandler for DisabledTipsetDataCache {
    fn insert_receipt(&self, _key: &TipsetKey, _receipts: Vec<Receipt>) {
        // No-op
    }

    fn insert_events(&self, _key: &TipsetKey, _events_data: StateEvents) {
        // No-op
    }

    fn get_events(&self, _key: &TipsetKey) -> Option<StateEvents> {
        None
    }

    fn get_receipts(&self, _key: &TipsetKey) -> Option<Vec<Receipt>> {
        None
    }

    fn get_receipt_or_else(
        &self,
        _key: &TipsetKey,
        _compute: ComputeReceiptFn,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<Receipt>>> + Send + '_>> {
        Box::pin(async move { Ok(vec![]) })
    }

    fn get_events_or_else(
        &self,
        _key: &TipsetKey,
        _compute: ComputeEventsFn,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<StateEvents>> + Send + '_>> {
        Box::pin(async move {
            Ok(StateEvents {
                events: vec![],
                roots: vec![],
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::TipsetKey;
    use crate::shim::executor::Receipt;
    use cid::Cid;
    use fvm_ipld_encoding::DAG_CBOR;
    use multihash_derive::MultihashDigest;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
    use std::time::Duration;

    fn create_test_tipset_key(i: u64) -> TipsetKey {
        let bytes = i.to_le_bytes().to_vec();
        let cid = Cid::new_v1(
            DAG_CBOR,
            crate::utils::multihash::MultihashCode::Blake2b256.digest(&bytes),
        );
        TipsetKey::from(nunny::vec![cid])
    }

    fn create_test_receipt(i: u64) -> Vec<Receipt> {
        vec![Receipt::V4(fvm_shared4::receipt::Receipt {
            exit_code: fvm_shared4::error::ExitCode::new(0),
            return_data: fvm_ipld_encoding::RawBytes::default(),
            gas_used: i * 100,
            events_root: None,
        })]
    }

    #[tokio::test]
    async fn test_tipset_cache_basic_functionality() {
        let cache: TipsetStateCache<String> = TipsetStateCache::new();
        let key = create_test_tipset_key(1);

        // Test cache miss and computation
        let result = cache
            .get_or_else(&key, || async { Ok("computed_value".to_string()) })
            .await
            .unwrap();
        assert_eq!(result, "computed_value");

        // Test cache hit
        let result = cache
            .get_or_else(&key, || async { Ok("should_not_compute".to_string()) })
            .await
            .unwrap();
        assert_eq!(result, "computed_value");
    }

    #[tokio::test]
    async fn test_concurrent_same_key_computation() {
        let cache: Arc<TipsetStateCache<String>> = Arc::new(TipsetStateCache::new());
        let key = create_test_tipset_key(1);
        let computation_count = Arc::new(AtomicU8::new(0));

        // Start multiple tasks that try to compute the same key concurrently
        let mut handles = vec![];
        for i in 0..10 {
            let cache_clone = Arc::clone(&cache);
            let key_clone = key.clone();
            let count_clone = Arc::clone(&computation_count);

            let handle = tokio::spawn(async move {
                cache_clone
                    .get_or_else(&key_clone, || {
                        let count = Arc::clone(&count_clone);
                        async move {
                            // Increment computation count
                            count.fetch_add(1, Ordering::SeqCst);
                            // Simulate some computation time
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            Ok(format!("computed_value_{i}"))
                        }
                    })
                    .await
            });
            handles.push(handle);
        }

        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        // Computation should have been performed once
        assert_eq!(computation_count.load(Ordering::SeqCst), 1);

        // Only one result should be returned as computation was performed once,
        // and all tasks will get the same result from the cache
        let first_result = results[0].as_ref().unwrap();
        for result in &results {
            assert_eq!(result.as_ref().unwrap(), first_result);
        }
    }

    #[tokio::test]
    async fn test_concurrent_different_keys() {
        let cache: Arc<TipsetStateCache<String>> = Arc::new(TipsetStateCache::new());
        let computation_count = Arc::new(AtomicU8::new(0));

        // Start tasks that try to compute the different keys
        let mut handles = vec![];
        for i in 0..10 {
            let cache_clone = Arc::clone(&cache);
            let key = create_test_tipset_key(i);
            let count_clone = Arc::clone(&computation_count);

            let handle = tokio::spawn(async move {
                cache_clone
                    .get_or_else(&key, || {
                        let count = Arc::clone(&count_clone);
                        async move {
                            count.fetch_add(1, Ordering::SeqCst);
                            tokio::time::sleep(Duration::from_millis(5)).await;
                            Ok(format!("value_{i}"))
                        }
                    })
                    .await
            });
            handles.push(handle);
        }

        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        // Computation should have been performed for each key
        assert_eq!(computation_count.load(Ordering::SeqCst), 10);

        // All results should be returned as computation was performed once for each key
        for (i, result) in results.iter().enumerate() {
            assert_eq!(result.as_ref().unwrap(), &format!("value_{i}"));
        }
    }

    #[tokio::test]
    async fn test_enabled_cache_concurrent_access() {
        let cache = Arc::new(EnabledTipsetDataCache::new());
        let key = create_test_tipset_key(1);
        let computation_count = Arc::new(AtomicU32::new(0));

        let mut handles = vec![];
        for i in 0..5 {
            let cache_clone = Arc::clone(&cache);
            let key_clone = key.clone();
            let count_clone = Arc::clone(&computation_count);

            let handle = tokio::spawn(async move {
                cache_clone
                    .get_receipt_or_else(
                        &key_clone,
                        Box::new(move || {
                            let count = Arc::clone(&count_clone);
                            Box::pin(async move {
                                count.fetch_add(1, Ordering::SeqCst);
                                tokio::time::sleep(Duration::from_millis(10)).await;
                                Ok(create_test_receipt(i))
                            })
                        }),
                    )
                    .await
            });
            handles.push(handle);
        }

        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        // Computation should have been performed once
        assert_eq!(computation_count.load(Ordering::SeqCst), 1);

        // Only one result should be returned as computation was performed once,
        // and all tasks will get the same result from the cache
        let first_result = results[0].as_ref().unwrap();
        for result in &results {
            let receipts = result.as_ref().unwrap();
            assert_eq!(receipts.len(), first_result.len());
        }
    }

    #[tokio::test]
    async fn test_disabled_cache_behavior() {
        let cache = Arc::new(DisabledTipsetDataCache::new());
        let key = create_test_tipset_key(1);
        let computation_count = Arc::new(AtomicU32::new(0));

        // Test that the disabled cache doesn't compute and returns empty results
        let mut handles = vec![];
        for i in 0..3 {
            let cache_clone = Arc::clone(&cache);
            let key_clone = key.clone();
            let count_clone = Arc::clone(&computation_count);

            let handle = tokio::spawn(async move {
                cache_clone
                    .get_receipt_or_else(
                        &key_clone,
                        Box::new(move || {
                            let count = Arc::clone(&count_clone);
                            Box::pin(async move {
                                count.fetch_add(1, Ordering::SeqCst);
                                Ok(create_test_receipt(i))
                            })
                        }),
                    )
                    .await
            });
            handles.push(handle);
        }

        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        // Disabled cache should never compute - it returns empty results immediately
        assert_eq!(computation_count.load(Ordering::SeqCst), 0);

        // All results should be empty
        for result in &results {
            let receipts = result.as_ref().unwrap();
            assert!(receipts.is_empty());
        }
    }
}
