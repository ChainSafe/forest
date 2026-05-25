// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{borrow::Cow, fmt::Debug, hash::Hash, num::NonZeroUsize, sync::atomic::Ordering};

use get_size2::GetSize;
use prometheus_client::{
    collector::Collector,
    encoding::{DescriptorEncoder, EncodeMetric},
    metrics::{counter::Counter, gauge::Gauge},
    registry::Unit,
};
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
#[derive(Debug, derive_more::Deref)]
pub struct SizeTrackingCache<K, V>
where
    K: CacheKeyConstraints,
    V: CacheValueConstraints,
{
    cache_name: Cow<'static, str>,
    #[deref]
    cache: Arc<Cache<K, V>>,
}

impl<K, V> ShallowClone for SizeTrackingCache<K, V>
where
    K: CacheKeyConstraints,
    V: CacheValueConstraints,
{
    fn shallow_clone(&self) -> Self {
        Self {
            cache_name: self.cache_name.clone(),
            cache: self.cache.shallow_clone(),
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
        Self {
            cache_name: cache_name.into(),
            cache: Arc::new(Cache::new(capacity.get())),
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

    /// Insert `k`/`v`. If a previous entry existed for `k`, return it.
    ///
    /// `quick_cache::sync::Cache::insert` does not return the displaced
    /// value, so this is a peek-then-insert. The two steps are not atomic;
    /// concurrent callers for the same key may both observe `None`. None of
    /// the existing callers depend on atomicity here.
    #[inline]
    pub fn push_and_get_prev(&self, k: K, v: V) -> Option<V> {
        let prev = self.cache.peek(&k);
        self.cache.insert(k, v);
        prev
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
            let size_metric_name = format!("cache_{}_size", self.cache_name);
            let size_metric_help = format!("Size of cache {} in bytes", self.cache_name);
            let size_metric_encoder = encoder.encode_descriptor(
                &size_metric_name,
                &size_metric_help,
                Some(&Unit::Bytes),
                size_in_bytes.metric_type(),
            )?;
            size_in_bytes.encode(size_metric_encoder)?;
        }
        {
            let len_metric_name = format!("cache_{}_len", self.cache_name);
            let len_metric_help = format!("Length of cache {}", self.cache_name);
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
            let cap_metric_name = format!("cache_{}_cap", self.cache_name);
            let cap_metric_help = format!("Capacity of cache {}", self.cache_name);
            let cap: Gauge = Default::default();
            cap.set(self.capacity() as _);
            let cap_metric_encoder = encoder.encode_descriptor(
                &cap_metric_name,
                &cap_metric_help,
                None,
                cap.metric_type(),
            )?;
            cap.encode(cap_metric_encoder)?;
        }
        {
            let hits_metric_name = format!("cache_{}_hits", self.cache_name);
            let hits_metric_help = format!("Cache hits of {}", self.cache_name);
            let hits: Counter = Default::default();
            hits.inner().store(self.cache.hits(), Ordering::Relaxed);
            let hits_metric_encoder = encoder.encode_descriptor(
                &hits_metric_name,
                &hits_metric_help,
                None,
                hits.metric_type(),
            )?;
            hits.encode(hits_metric_encoder)?;
        }
        {
            let misses_metric_name = format!("cache_{}_misses", self.cache_name);
            let misses_metric_help = format!("Cache misses of {}", self.cache_name);
            let misses: Counter = Default::default();
            misses.inner().store(self.cache.misses(), Ordering::Relaxed);
            let misses_metric_encoder = encoder.encode_descriptor(
                &misses_metric_name,
                &misses_metric_help,
                None,
                misses.metric_type(),
            )?;
            misses.encode(misses_metric_encoder)?;
        }

        Ok(())
    }
}
