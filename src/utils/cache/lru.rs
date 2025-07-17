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
use lru::LruCache;
use parking_lot::RwLock;
use prometheus_client::{
    collector::Collector,
    encoding::{DescriptorEncoder, EncodeMetric},
    metrics::gauge::Gauge,
    registry::Registry,
    registry::Unit,
};

use crate::metrics::default_registry;

#[derive(Debug, Clone)]
pub struct SizeTrackingLruCache<K, V>
where
    K: GetSize + Debug + Send + Sync + Hash + PartialEq + Eq + Clone + 'static,
    V: GetSize + Debug + Send + Sync + Clone + 'static,
{
    cache_id: usize,
    cache_name: Cow<'static, str>,
    cache: Arc<RwLock<LruCache<K, V>>>,
    size_in_bytes: Gauge,
}

impl<K, V> SizeTrackingLruCache<K, V>
where
    K: GetSize + Debug + Send + Sync + Hash + PartialEq + Eq + Clone + 'static,
    V: GetSize + Debug + Send + Sync + Clone + 'static,
{
    pub fn register_metrics(&self, registry: &mut Registry) {
        registry.register_collector(Box::new(self.clone()));
    }

    pub fn new_without_metrics_registry(
        cache_name: Cow<'static, str>,
        capacity: NonZeroUsize,
    ) -> Self {
        static ID_GENERATOR: AtomicUsize = AtomicUsize::new(0);

        Self {
            cache_id: ID_GENERATOR.fetch_add(1, Ordering::Relaxed),
            cache_name,
            cache: Arc::new(RwLock::new(LruCache::new(capacity))),
            size_in_bytes: Default::default(),
        }
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

    pub fn push(&self, k: K, v: V) -> Option<(K, V)> {
        self.size_in_bytes
            .inc_by(k.get_size().saturating_add(v.get_size()) as _);
        let old = self.cache.write().push(k, v);
        if let Some((old_k, old_v)) = &old {
            self.size_in_bytes
                .dec_by(old_k.get_size().saturating_add(old_v.get_size()) as _);
        }
        old
    }

    pub fn get_cloned<Q>(&self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.cache.write().get(k).cloned()
    }

    pub fn len(&self) -> usize {
        self.cache.read().len()
    }

    pub fn size_in_bytes(&self) -> usize {
        self.size_in_bytes.get() as _
    }
}

impl<K, V> Collector for SizeTrackingLruCache<K, V>
where
    K: GetSize + Debug + Send + Sync + Hash + PartialEq + Eq + Clone + 'static,
    V: GetSize + Debug + Send + Sync + Clone + 'static,
{
    fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
        {
            let size_metric_name = format!("{}_{}_size", self.cache_name, self.cache_id);
            let size_metric_help = format!(
                "Size of LruCache {}_{} in bytes",
                self.cache_name, self.cache_id
            );
            let size_metric_encoder = encoder.encode_descriptor(
                &size_metric_name,
                &size_metric_help,
                Some(&Unit::Bytes),
                self.size_in_bytes.metric_type(),
            )?;
            self.size_in_bytes.encode(size_metric_encoder)?;
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
            cap.set(self.cache.read().cap().get() as _);
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
