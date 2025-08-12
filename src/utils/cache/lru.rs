// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    borrow::{Borrow, Cow},
    fmt::Debug,
    hash::Hash,
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use get_size2::GetSize;
use hashlink::LruCache;
use parking_lot::RwLock;
use prometheus_client::{
    collector::Collector,
    encoding::{DescriptorEncoder, EncodeMetric},
    metrics::gauge::Gauge,
    registry::Registry,
    registry::Unit,
};

use crate::metrics::default_registry;

pub trait KeyConstraints:
    GetSize + Debug + Send + Sync + Hash + PartialEq + Eq + Clone + 'static
{
}

impl<T> KeyConstraints for T where
    T: GetSize + Debug + Send + Sync + Hash + PartialEq + Eq + Clone + 'static
{
}

pub trait LruValueConstraints: GetSize + Debug + Send + Sync + Clone + 'static {}

impl<T> LruValueConstraints for T where T: GetSize + Debug + Send + Sync + Clone + 'static {}

#[derive(Debug, Clone)]
pub struct SizeTrackingLruCache<K, V>
where
    K: KeyConstraints,
    V: LruValueConstraints,
{
    cache_id: usize,
    cache_name: Cow<'static, str>,
    cache: Arc<RwLock<LruCache<K, V>>>,
}

impl<K, V> SizeTrackingLruCache<K, V>
where
    K: KeyConstraints,
    V: LruValueConstraints,
{
    pub fn register_metrics(&self, registry: &mut Registry) {
        registry.register_collector(Box::new(self.clone()));
    }

    fn new_inner(cache_name: Cow<'static, str>, capacity: Option<NonZeroUsize>) -> Self {
        static ID_GENERATOR: AtomicUsize = AtomicUsize::new(0);

        Self {
            cache_id: ID_GENERATOR.fetch_add(1, Ordering::Relaxed),
            cache_name,
            #[allow(clippy::disallowed_methods)]
            cache: Arc::new(RwLock::new(
                capacity
                    .map(From::from)
                    .map(LruCache::new)
                    // For constructing lru cache that is bounded by memory usage instead of length
                    .unwrap_or_else(LruCache::new_unbounded),
            )),
        }
    }

    pub fn new_without_metrics_registry(
        cache_name: Cow<'static, str>,
        capacity: NonZeroUsize,
    ) -> Self {
        Self::new_inner(cache_name, Some(capacity))
    }

    pub fn new_with_metrics_registry(
        cache_name: Cow<'static, str>,
        capacity: NonZeroUsize,
        metrics_registry: &mut Registry,
    ) -> Self {
        let c = Self::new_without_metrics_registry(cache_name, capacity);
        c.register_metrics(metrics_registry);
        c
    }

    pub fn new_with_default_metrics_registry(
        cache_name: Cow<'static, str>,
        capacity: NonZeroUsize,
    ) -> Self {
        Self::new_with_metrics_registry(cache_name, capacity, &mut default_registry())
    }

    pub fn unbounded_without_metrics_registry(cache_name: Cow<'static, str>) -> Self {
        Self::new_inner(cache_name, None)
    }

    pub fn unbounded_with_metrics_registry(
        cache_name: Cow<'static, str>,
        metrics_registry: &mut Registry,
    ) -> Self {
        let c = Self::unbounded_without_metrics_registry(cache_name);
        c.register_metrics(metrics_registry);
        c
    }

    pub fn unbounded_with_default_metrics_registry(cache_name: Cow<'static, str>) -> Self {
        Self::unbounded_with_metrics_registry(cache_name, &mut default_registry())
    }

    pub fn cache(&self) -> &Arc<RwLock<LruCache<K, V>>> {
        &self.cache
    }

    pub fn push(&self, k: K, v: V) -> Option<V> {
        self.cache.write().insert(k, v)
    }

    pub fn contains<Q>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.cache.read().contains(k)
    }

    pub fn get_cloned<Q>(&self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.cache.write().get(k).cloned()
    }

    pub fn peek_cloned<Q>(&self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.cache.read().peek(k).cloned()
    }

    pub fn pop_lru(&self) -> Option<(K, V)> {
        self.cache.write().remove_lru()
    }

    pub fn len(&self) -> usize {
        self.cache.read().len()
    }

    pub fn cap(&self) -> usize {
        self.cache.read().capacity()
    }

    pub(crate) fn size_in_bytes(&self) -> usize {
        let mut size = 0_usize;
        for (k, v) in self.cache.read().iter() {
            size = size
                .saturating_add(k.get_size())
                .saturating_add(v.get_size());
        }
        size
    }
}

impl<K, V> Collector for SizeTrackingLruCache<K, V>
where
    K: KeyConstraints,
    V: LruValueConstraints,
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
                "Size of LruCache {}_{} in bytes",
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
            let len_metric_help =
                format!("Length of LruCache {}_{}", self.cache_name, self.cache_id);
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
                format!("Capacity of LruCache {}_{}", self.cache_name, self.cache_id);
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
