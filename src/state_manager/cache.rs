// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::blocks::TipsetKey;
use crate::state_manager::DEFAULT_TIPSET_CACHE_SIZE;
use crate::utils::cache::{LruValueConstraints, SizeTrackingLruCache};
use parking_lot::Mutex as SyncMutex;
use std::future::Future;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

struct TipsetStateCacheInner<V: LruValueConstraints> {
    values: SizeTrackingLruCache<TipsetKey, V>,
    pending: Vec<(TipsetKey, Arc<TokioMutex<()>>)>,
}

impl<V: LruValueConstraints> TipsetStateCacheInner<V> {
    pub fn with_size(cache_identifier: &str, cache_size: NonZeroUsize) -> Self {
        Self {
            values: SizeTrackingLruCache::new_with_metrics(
                Self::cache_name(cache_identifier).into(),
                cache_size,
            ),
            pending: Vec::with_capacity(8),
        }
    }

    fn cache_name(cache_identifier: &str) -> String {
        format!("tipset_state_{cache_identifier}")
    }
}

/// A generic cache that handles concurrent access and computation for tipset-related data.
pub(crate) struct TipsetStateCache<V: LruValueConstraints> {
    cache: Arc<SyncMutex<TipsetStateCacheInner<V>>>,
}

enum CacheLookupStatus<V> {
    Exist(V),
    Empty(Arc<TokioMutex<()>>),
}

impl<V: LruValueConstraints> TipsetStateCache<V> {
    pub fn new(cache_identifier: &str) -> Self {
        Self::with_size(cache_identifier, DEFAULT_TIPSET_CACHE_SIZE)
    }

    pub fn with_size(cache_identifier: &str, cache_size: NonZeroUsize) -> Self {
        Self {
            cache: Arc::new(SyncMutex::new(TipsetStateCacheInner::with_size(
                cache_identifier,
                cache_size,
            ))),
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
        let status = self.with_inner(|inner| match inner.values.get_cloned(key) {
            Some(v) => CacheLookupStatus::Exist(v),
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

    pub fn get_map<T>(&self, key: &TipsetKey, mapper: impl Fn(&V) -> T) -> Option<T> {
        self.with_inner(|inner| inner.values.get_map(key, mapper))
    }

    pub fn get(&self, key: &TipsetKey) -> Option<V> {
        self.get_map(key, Clone::clone)
    }

    pub fn insert(&self, key: TipsetKey, value: V) {
        self.with_inner(|inner| {
            inner.pending.retain(|(k, _)| k != &key);
            inner.values.push(key, value);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::TipsetKey;
    use cid::Cid;
    use fvm_ipld_encoding::DAG_CBOR;
    use multihash_derive::MultihashDigest;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::time::Duration;

    fn create_test_tipset_key(i: u64) -> TipsetKey {
        let bytes = i.to_le_bytes().to_vec();
        let cid = Cid::new_v1(
            DAG_CBOR,
            crate::utils::multihash::MultihashCode::Blake2b256.digest(&bytes),
        );
        TipsetKey::from(nunny::vec![cid])
    }

    #[tokio::test]
    async fn test_tipset_cache_basic_functionality() {
        let cache: TipsetStateCache<String> = TipsetStateCache::new("test");
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
        let cache: Arc<TipsetStateCache<String>> = Arc::new(TipsetStateCache::new("test"));
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
        let cache: Arc<TipsetStateCache<String>> = Arc::new(TipsetStateCache::new("test"));
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
}
