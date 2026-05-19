// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    borrow::Cow,
    fmt::Debug,
    hash::Hash,
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use get_size2::GetSize;
use prometheus_client::{
    collector::Collector,
    encoding::{DescriptorEncoder, EncodeMetric},
    metrics::gauge::Gauge,
    registry::Unit,
};
use quick_cache::Equivalent;
use quick_cache::sync::Cache;

use crate::prelude::*;

pub trait CacheKeyConstraints:
    GetSize + Debug + Send + Sync + Hash + PartialEq + Eq + Clone + 'static
{
}

impl<T> CacheKeyConstraints for T where
    T: GetSize + Debug + Send + Sync + Hash + PartialEq + Eq + Clone + 'static
{
}

pub trait CacheValueConstraints: GetSize + Debug + Send + Sync + Clone + 'static {}

impl<T> CacheValueConstraints for T where T: GetSize + Debug + Send + Sync + Clone + 'static {}

/// A concurrent cache with Prometheus instrumentation.
///
/// Backed by [`quick_cache::sync::Cache`], which uses the scan-resistant
/// CLOCK-PRO eviction policy. Tracks total entry size in bytes for
/// observability.
#[derive(Debug)]
pub struct SizeTrackingCache<K, V>
where
    K: CacheKeyConstraints,
    V: CacheValueConstraints,
{
    cache_id: usize,
    cache_name: Cow<'static, str>,
    cache: Arc<Cache<K, V>>,
    capacity: usize,
}

impl<K, V> ShallowClone for SizeTrackingCache<K, V>
where
    K: CacheKeyConstraints,
    V: CacheValueConstraints,
{
    fn shallow_clone(&self) -> Self {
        Self {
            cache_id: self.cache_id,
            cache_name: self.cache_name.clone(),
            cache: self.cache.shallow_clone(),
            capacity: self.capacity,
        }
    }
}

impl<K, V> SizeTrackingCache<K, V>
where
    K: CacheKeyConstraints,
    V: CacheValueConstraints,
{
    fn register_metrics(&self) {
        crate::metrics::register_collector(Box::new(self.shallow_clone()));
    }

    fn new_inner(cache_name: impl Into<Cow<'static, str>>, capacity: NonZeroUsize) -> Self {
        static ID_GENERATOR: AtomicUsize = AtomicUsize::new(0);
        let capacity = capacity.get();
        Self {
            cache_id: ID_GENERATOR.fetch_add(1, Ordering::Relaxed),
            cache_name: cache_name.into(),
            cache: Arc::new(Cache::new(capacity)),
            capacity,
        }
    }

    pub fn new_without_metrics_registry(
        cache_name: impl Into<Cow<'static, str>>,
        capacity: NonZeroUsize,
    ) -> Self {
        Self::new_inner(cache_name, capacity)
    }

    pub fn new_with_metrics(
        cache_name: impl Into<Cow<'static, str>>,
        capacity: NonZeroUsize,
    ) -> Self {
        let c = Self::new_without_metrics_registry(cache_name, capacity);
        c.register_metrics();
        c
    }

    #[inline]
    pub fn remove<Q>(&self, k: &Q) -> Option<V>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.cache.remove(k).map(|(_, v)| v)
    }

    /// Insert `k`/`v`. If a previous entry existed for `k`, return it.
    ///
    /// `quick_cache::sync::Cache::insert` does not return the displaced
    /// value, so this is a peek-then-insert. The two steps are not atomic;
    /// concurrent callers for the same key may both observe `None`. None of
    /// the existing callers depend on atomicity here.
    #[inline]
    pub fn push(&self, k: K, v: V) -> Option<V> {
        let prev = self.cache.peek(&k);
        self.cache.insert(k, v);
        prev
    }

    #[inline]
    pub fn get_map<Q, T>(&self, k: &Q, mapper: impl FnOnce(&V) -> T) -> Option<T>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.cache.get(k).map(|v| mapper(&v))
    }

    #[inline]
    pub fn get_cloned<Q>(&self, k: &Q) -> Option<V>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.cache.get(k)
    }

    /// Read without affecting the eviction order.
    #[inline]
    pub fn peek_cloned<Q>(&self, k: &Q) -> Option<V>
    where
        Q: Hash + Equivalent<K> + ?Sized,
    {
        self.cache.peek(k)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    #[inline]
    pub fn cap(&self) -> usize {
        self.capacity
    }

    #[inline]
    pub fn clear(&self) {
        self.cache.clear()
    }

    /// Gets or inserts an item in the cache with key.
    /// Concurrent callers for the same key are coalesced — only one runs
    #[inline]
    pub fn get_or_insert_with<Q, E>(
        &self,
        key: &Q,
        with: impl FnOnce() -> Result<V, E>,
    ) -> Result<V, E>
    where
        Q: Hash + Equivalent<K> + ToOwned<Owned = K> + ?Sized,
    {
        self.cache.get_or_insert_with(key, with)
    }

    /// Gets or inserts an item in the cache with key. Async version.
    /// Concurrent callers for the same key are coalesced — only one runs
    #[inline]
    pub async fn get_or_insert_async<Q, E>(
        &self,
        key: &Q,
        with: impl Future<Output = Result<V, E>>,
    ) -> Result<V, E>
    where
        Q: Hash + Equivalent<K> + ToOwned<Owned = K> + ?Sized,
    {
        self.cache.get_or_insert_async(key, with).await
    }

    pub(crate) fn size_in_bytes(&self) -> usize {
        let mut size = 0_usize;
        for (k, v) in self.cache.iter() {
            size = size
                .saturating_add(k.get_size())
                .saturating_add(v.get_size());
        }
        size
    }
}

impl<K, V> Collector for SizeTrackingCache<K, V>
where
    K: CacheKeyConstraints,
    V: CacheValueConstraints,
{
    fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
        {
            let size_in_bytes = {
                let g: Gauge = Default::default();
                g.set(self.size_in_bytes() as _);
                g
            };
            let size_metric_name = format!("cache_{}_{}_size", self.cache_name, self.cache_id);
            let size_metric_help = format!(
                "Size of cache {}_{} in bytes",
                self.cache_name, self.cache_id
            );
            let size_metric_encoder = encoder.encode_descriptor(
                &size_metric_name,
                &size_metric_help,
                Some(&Unit::Bytes),
                size_in_bytes.metric_type(),
            )?;
            size_in_bytes.encode(size_metric_encoder)?;
        }
        {
            let len_metric_name = format!("{}_{}_len", self.cache_name, self.cache_id);
            let len_metric_help = format!("Length of cache {}_{}", self.cache_name, self.cache_id);
            let len: Gauge = Default::default();
            len.set(self.len() as _);
            let len_metric_encoder = encoder.encode_descriptor(
                &len_metric_name,
                &len_metric_help,
                None,
                len.metric_type(),
            )?;
            len.encode(len_metric_encoder)?;
        }
        {
            let cap_metric_name = format!("{}_{}_cap", self.cache_name, self.cache_id);
            let cap_metric_help =
                format!("Capacity of cache {}_{}", self.cache_name, self.cache_id);
            let cap: Gauge = Default::default();
            cap.set(self.cap() as _);
            let cap_metric_encoder = encoder.encode_descriptor(
                &cap_metric_name,
                &cap_metric_help,
                None,
                cap.metric_type(),
            )?;
            cap.encode(cap_metric_encoder)?;
        }

        Ok(())
    }
}
