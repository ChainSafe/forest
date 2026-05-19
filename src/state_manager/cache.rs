// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::prelude::*;
use crate::state_manager::DEFAULT_TIPSET_CACHE_SIZE;
use crate::utils::cache::{CacheKeyConstraints, CacheValueConstraints, SizeTrackingCache};
use std::borrow::Cow;
use std::future::Future;
use std::num::NonZeroUsize;

/// A cache that handles concurrent access and computation for tipset-related
/// data. Coalesces concurrent computations of the same key, so only one caller
/// actually runs the `compute` future and the rest wait on its result.
pub(crate) struct ForestCache<K: CacheKeyConstraints, V: CacheValueConstraints> {
    cache: SizeTrackingCache<K, V>,
}

impl<K: CacheKeyConstraints, V: CacheValueConstraints> ShallowClone for ForestCache<K, V> {
    fn shallow_clone(&self) -> Self {
        Self {
            cache: self.cache.shallow_clone(),
        }
    }
}

impl<K: CacheKeyConstraints, V: CacheValueConstraints> ForestCache<K, V> {
    pub fn new(cache_identifier: impl Into<Cow<'static, str>>) -> Self {
        Self::with_size(cache_identifier, DEFAULT_TIPSET_CACHE_SIZE)
    }

    pub fn with_size(
        cache_identifier: impl Into<Cow<'static, str>>,
        cache_size: NonZeroUsize,
    ) -> Self {
        Self {
            cache: SizeTrackingCache::new_with_metrics(cache_identifier, cache_size),
        }
    }

    pub async fn get_or_else<F, Fut>(&self, key: &K, compute: F) -> anyhow::Result<V>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = anyhow::Result<V>> + Send,
        V: Send + Sync + 'static,
    {
        self.cache.get_or_insert_async(key, compute()).await
    }

    pub fn get_map<T>(&self, key: &K, mapper: impl FnOnce(&V) -> T) -> Option<T> {
        self.cache.get_map(key, mapper)
    }

    pub fn get(&self, key: &K) -> Option<V> {
        self.cache.get_cloned(key)
    }

    pub fn remove(&self, key: &K) {
        self.cache.remove(key);
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
        let cache: ForestCache<TipsetKey, String> = ForestCache::new("test");
        let key = create_test_tipset_key(1);

        let result = cache
            .get_or_else(&key, || async { Ok("computed_value".to_string()) })
            .await
            .unwrap();
        assert_eq!(result, "computed_value");

        let result = cache
            .get_or_else(&key, || async { Ok("should_not_compute".to_string()) })
            .await
            .unwrap();
        assert_eq!(result, "computed_value");
    }

    #[tokio::test]
    async fn test_concurrent_same_key_computation() {
        let cache: Arc<ForestCache<TipsetKey, String>> = Arc::new(ForestCache::new("test"));
        let key = create_test_tipset_key(1);
        let computation_count = Arc::new(AtomicU8::new(0));

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
                            count.fetch_add(1, Ordering::SeqCst);
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

        assert_eq!(computation_count.load(Ordering::SeqCst), 1);

        let first_result = results[0].as_ref().unwrap();
        for result in &results {
            assert_eq!(result.as_ref().unwrap(), first_result);
        }
    }

    #[tokio::test]
    async fn test_concurrent_different_keys() {
        let cache: Arc<ForestCache<TipsetKey, String>> = Arc::new(ForestCache::new("test"));
        let computation_count = Arc::new(AtomicU8::new(0));

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

        assert_eq!(computation_count.load(Ordering::SeqCst), 10);

        for (i, result) in results.iter().enumerate() {
            assert_eq!(result.as_ref().unwrap(), &format!("value_{i}"));
        }
    }
}
