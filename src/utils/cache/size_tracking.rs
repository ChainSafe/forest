// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    borrow::Cow,
    fmt::Debug,
    hash::Hash,
    num::NonZeroUsize,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use get_size2::GetSize;
use parking_lot::Mutex;
use prometheus_client::{
    collector::Collector,
    encoding::{DescriptorEncoder, EncodeMetric},
    metrics::{counter::Counter, gauge::Gauge},
    registry::Unit,
};
use quick_cache::sync::Cache;

use crate::prelude::*;

/// [`SizeTrackingCache::size_in_bytes`] clones every entry and deep-walks it, which for caches of
/// large values costs more than a scrape should. The gauge is an observability aid, so a stale
/// reading is preferable to making every scrape pay that. Kept well above a typical 15-30s scrape
/// interval, or the memo expires before every scrape and buys nothing.
const SIZE_GAUGE_MAX_STALENESS: Duration = Duration::from_secs(300);

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
    size_in_bytes_memo: Arc<Mutex<Option<(Instant, usize)>>>,
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
            size_in_bytes_memo: self.size_in_bytes_memo.shallow_clone(),
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
            size_in_bytes_memo: Default::default(),
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
        let mut memo = self.size_in_bytes_memo.lock();
        // Covers `clear` and any other drain: an emptied cache must not keep reporting its old total.
        if self.cache.is_empty() {
            *memo = None;
            return 0;
        }
        if let Some((measured_at, size)) = *memo
            && measured_at.elapsed() < SIZE_GAUGE_MAX_STALENESS
        {
            return size;
        }
        let mut size = 0_usize;
        for (k, v) in self.cache.iter() {
            size = size
                .saturating_add(k.get_size())
                .saturating_add(v.get_size());
        }
        *memo = Some((Instant::now(), size));
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

#[cfg(test)]
mod tests {
    use super::*;
    use nonzero_ext::nonzero;

    #[test]
    fn size_gauge_is_memoized_until_cleared() {
        let cache: SizeTrackingCache<u64, Vec<u8>> =
            SizeTrackingCache::new_without_metrics_registry("test_size_memo", nonzero!(64usize));
        cache.insert(1, vec![0; 1024]);
        let first = cache.size_in_bytes();
        assert!(first >= 1024);

        // Within the staleness window the walk is skipped, so growth is not reflected yet.
        cache.insert(2, vec![0; 1024]);
        assert_eq!(cache.size_in_bytes(), first);

        // `clear` drops the memo, so the gauge does not keep reporting a pre-clear total.
        cache.clear();
        assert_eq!(cache.size_in_bytes(), 0);
    }
}
