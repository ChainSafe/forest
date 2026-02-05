// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod db;

use crate::{db::DBStatistics, networks::ChainConfig, shim::clock::ChainEpoch};
use axum::{Router, http::StatusCode, response::IntoResponse, routing::get};
use parking_lot::{RwLock, RwLockWriteGuard};
use prometheus_client::{
    collector::Collector,
    encoding::EncodeLabelSet,
    metrics::{
        counter::Counter,
        family::Family,
        histogram::{Histogram, exponential_buckets},
    },
};
use std::sync::{Arc, LazyLock};
use std::{path::PathBuf, time::Instant};
use tokio::net::TcpListener;
use tower_http::compression::CompressionLayer;
use tracing::warn;

static DEFAULT_REGISTRY: LazyLock<RwLock<prometheus_client::registry::Registry>> =
    LazyLock::new(Default::default);

static COLLECTOR_REGISTRY: LazyLock<RwLock<prometheus_client::registry::Registry>> =
    LazyLock::new(Default::default);

pub fn default_registry<'a>() -> RwLockWriteGuard<'a, prometheus_client::registry::Registry> {
    DEFAULT_REGISTRY.write()
}

pub fn collector_registry<'a>() -> RwLockWriteGuard<'a, prometheus_client::registry::Registry> {
    COLLECTOR_REGISTRY.write()
}

pub fn register_collector(collector: Box<dyn Collector>) {
    #[allow(clippy::disallowed_methods)]
    collector_registry().register_collector(collector)
}

pub fn reset_collector_registry() {
    *collector_registry() = Default::default();
}

pub static LRU_CACHE_HIT: LazyLock<Family<KindLabel, Counter>> = LazyLock::new(|| {
    let metric = Family::default();
    DEFAULT_REGISTRY
        .write()
        .register("lru_cache_hit", "Stats of lru cache hit", metric.clone());
    metric
});
pub static LRU_CACHE_MISS: LazyLock<Family<KindLabel, Counter>> = LazyLock::new(|| {
    let metric = Family::default();
    DEFAULT_REGISTRY
        .write()
        .register("lru_cache_miss", "Stats of lru cache miss", metric.clone());
    metric
});

pub static RPC_METHOD_FAILURE: LazyLock<Family<RpcMethodLabel, Counter>> = LazyLock::new(|| {
    let metric = Family::default();
    DEFAULT_REGISTRY.write().register(
        "rpc_method_failure",
        "Number of failed RPC calls",
        metric.clone(),
    );
    metric
});

pub static RPC_METHOD_TIME: LazyLock<Family<RpcMethodLabel, Histogram>> = LazyLock::new(|| {
    let metric = Family::<RpcMethodLabel, Histogram>::new_with_constructor(|| {
        // Histogram with 5 buckets starting from 0.1ms going to 1s, each bucket 10 times as big as the last.
        Histogram::new(exponential_buckets(0.1, 10., 5))
    });
    crate::metrics::default_registry().register(
        "rpc_processing_time",
        "Duration of RPC method call in milliseconds",
        metric.clone(),
    );
    metric
});

pub async fn init_prometheus<DB>(
    prometheus_listener: TcpListener,
    db_directory: PathBuf,
    db: Arc<DB>,
    chain_config: Arc<ChainConfig>,
    get_chain_head_height: Arc<impl Fn() -> ChainEpoch + Send + Sync + 'static>,
    get_chain_head_actor_version: Arc<impl Fn() -> u64 + Send + Sync + 'static>,
) -> anyhow::Result<()>
where
    DB: DBStatistics + Send + Sync + 'static,
{
    // Add the process collector to the registry
    if let Err(err) = kubert_prometheus_process::register(
        collector_registry().sub_registry_with_prefix("process"),
    ) {
        warn!("Failed to register process metrics: {err}");
    }

    register_collector(Box::new(
        crate::utils::version::ForestVersionCollector::new(),
    ));
    register_collector(Box::new(crate::metrics::db::DBCollector::new(db_directory)));
    register_collector(Box::new(
        crate::networks::metrics::NetworkVersionCollector::new(
            chain_config,
            get_chain_head_height,
            get_chain_head_actor_version,
        ),
    ));

    // Create an configure HTTP server
    let app = Router::new()
        .route("/metrics", get(collect_prometheus_metrics))
        .route("/stats/db", get(collect_db_metrics::<DB>))
        .layer(CompressionLayer::new())
        .with_state(db);

    // Wait for server to exit
    Ok(axum::serve(prometheus_listener, app.into_make_service()).await?)
}

async fn collect_prometheus_metrics() -> impl IntoResponse {
    let mut metrics = String::new();
    if let Err(e) =
        prometheus_client::encoding::text::encode_registry(&mut metrics, &DEFAULT_REGISTRY.read())
    {
        warn!("failed to encode the default metrics registry: {e}");
    };
    if let Err(e) =
        prometheus_client::encoding::text::encode_registry(&mut metrics, &COLLECTOR_REGISTRY.read())
    {
        warn!("failed to encode the collector metrics registry: {e}");
    };
    if let Err(e) = prometheus_client::encoding::text::encode_eof(&mut metrics) {
        warn!("failed to encode metrics eof {e}");
    };
    (
        StatusCode::OK,
        [("content-type", "text/plain; charset=utf-8")],
        metrics,
    )
}

async fn collect_db_metrics<DB>(
    axum::extract::State(db): axum::extract::State<Arc<DB>>,
) -> impl IntoResponse
where
    DB: DBStatistics,
{
    let mut metrics = "# DB statistics:\n".to_owned();
    if let Some(db_stats) = db.get_statistics() {
        metrics.push_str(&db_stats);
    } else {
        metrics.push_str("Not enabled. Set enable_statistics to true in config and restart daemon");
    }
    (
        StatusCode::OK,
        [("content-type", "text/plain; charset=utf-8")],
        metrics,
    )
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct RpcMethodLabel {
    pub method: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet, derive_more::Constructor)]
pub struct KindLabel {
    kind: &'static str,
}

pub mod values {
    use super::KindLabel;

    /// `TipsetCache`.
    pub const TIPSET: KindLabel = KindLabel::new("tipset");
    /// tipset cache in state manager
    pub const STATE_MANAGER_TIPSET: KindLabel = KindLabel::new("sm_tipset");
}

pub fn default_histogram() -> Histogram {
    // Default values from go client(https://github.com/prometheus/client_golang/blob/5d584e2717ef525673736d72cd1d12e304f243d7/prometheus/histogram.go#L68)
    Histogram::new([
        0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
    ])
}

pub struct HistogramTimer<'a> {
    histogram: &'a Histogram,
    start: Instant,
}

impl Drop for HistogramTimer<'_> {
    fn drop(&mut self) {
        let duration = Instant::now() - self.start;
        self.histogram.observe(duration.as_secs_f64());
    }
}

pub trait HistogramTimerExt {
    fn start_timer(&self) -> HistogramTimer<'_>;
}

impl HistogramTimerExt for Histogram {
    fn start_timer(&self) -> HistogramTimer<'_> {
        HistogramTimer {
            histogram: self,
            start: Instant::now(),
        }
    }
}
