// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::prelude::*;
use crate::state_manager::DEFAULT_TIPSET_CACHE_SIZE;
use crate::utils::cache::{LruKeyConstraints, LruValueConstraints, SizeTrackingLruCache};
use ahash::{HashMap, HashMapExt as _};
use parking_lot::RwLock as SyncRwLock;
use std::borrow::Cow;
use std::future::Future;
use std::num::NonZeroUsize;
use tokio::sync::Mutex as TokioMutex;

struct ForestLruCacheInner<K: LruKeyConstraints, V: LruValueConstraints> {
    values: SizeTrackingLruCache<K, V>,
    pending: HashMap<K, Arc<TokioMutex<()>>>,
}

impl<K: LruKeyConstraints, V: LruValueConstraints> ForestLruCacheInner<K, V> {
    pub fn with_size(
        cache_identifier: impl Into<Cow<'static, str>>,
        cache_size: NonZeroUsize,
    ) -> Self {
        Self {
            values: SizeTrackingLruCache::new_with_metrics(cache_identifier, cache_size),
            pending: HashMap::with_capacity(8),
        }
    }
}

/// A generic cache that handles concurrent access and computation for tipset-related data.
pub(crate) struct ForestLruCache<K: LruKeyConstraints, V: LruValueConstraints> {
    cache: Arc<SyncRwLock<ForestLruCacheInner<K, V>>>,
}

impl<K: LruKeyConstraints, V: LruValueConstraints> ShallowClone for ForestLruCache<K, V> {
    fn shallow_clone(&self) -> Self {
        Self {
            cache: self.cache.shallow_clone(),
        }
    }
}

enum CacheLookupStatus<V> {
    Exist(V),
    Empty(Arc<TokioMutex<()>>),
}

impl<K: LruKeyConstraints, V: LruValueConstraints> ForestLruCache<K, V> {
    pub fn new(cache_identifier: impl Into<Cow<'static, str>>) -> Self {
        Self::with_size(cache_identifier, DEFAULT_TIPSET_CACHE_SIZE)
    }

    pub fn with_size(
        cache_identifier: impl Into<Cow<'static, str>>,
        cache_size: NonZeroUsize,
    ) -> Self {
        Self {
            cache: Arc::new(SyncRwLock::new(ForestLruCacheInner::with_size(
                cache_identifier,
                cache_size,
            ))),
        }
    }

    fn get_or_insert<F1, F2, T>(&self, get_func: F1, or_insert_func: F2) -> T
    where
        F1: FnOnce(&ForestLruCacheInner<K, V>) -> Option<T>,
        F2: FnOnce(&mut ForestLruCacheInner<K, V>) -> T,
    {
        if let Some(v) = get_func(&self.cache.read()) {
            v
        } else {
            or_insert_func(&mut self.cache.write())
        }
    }

    pub async fn get_or_else<F, Fut>(&self, key: &K, compute: F) -> anyhow::Result<V>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = anyhow::Result<V>> + Send,
        V: Send + Sync + 'static,
    {
        let status = self.get_or_insert(
            |inner| inner.values.get_cloned(key).map(CacheLookupStatus::Exist),
            |inner| {
                let mutex = inner
                    .pending
                    .entry(key.clone())
                    .or_insert_with(|| Arc::new(TokioMutex::new(())))
                    .shallow_clone();
                CacheLookupStatus::Empty(mutex)
            },
        );
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

    pub fn get_map<T>(&self, key: &K, mapper: impl Fn(&V) -> T) -> Option<T> {
        self.cache.read().values.get_map(key, mapper)
    }

    pub fn get(&self, key: &K) -> Option<V> {
        self.get_map(key, Clone::clone)
    }

    pub fn insert(&self, key: K, value: V) {
        let mut cache = self.cache.write();
        cache.pending.retain(|k, _| k != &key);
        cache.values.push(key, value);
    }

    pub fn remove(&self, key: &K) {
        let mut cache = self.cache.write();
        cache.pending.retain(|k, _| k != key);
        cache.values.remove(key);
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
        let cache: ForestLruCache<TipsetKey, String> = ForestLruCache::new("test");
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
        let cache: Arc<ForestLruCache<TipsetKey, String>> = Arc::new(ForestLruCache::new("test"));
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
        let cache: Arc<ForestLruCache<TipsetKey, String>> = Arc::new(ForestLruCache::new("test"));
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
