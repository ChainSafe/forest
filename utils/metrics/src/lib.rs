// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod db;
pub mod metrics;

use std::{net::TcpListener, path::PathBuf};

use ahash::{HashMap, HashMapExt};
use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};
use forest_db::DBStatistics;
use log::warn;
use prometheus::{Encoder, TextEncoder};
use tokio::sync::RwLock;

lazy_static::lazy_static! {
    static ref REGISTRIES_EXT: RwLock<HashMap<String,prometheus_client::registry::Registry>> = RwLock::new(HashMap::new());
}

pub async fn add_metrics_registry(name: String, registry: prometheus_client::registry::Registry) {
    REGISTRIES_EXT.write().await.insert(name, registry);
}

pub async fn init_prometheus<DB>(
    prometheus_listener: TcpListener,
    db_directory: PathBuf,
    db: DB,
) -> anyhow::Result<()>
where
    DB: DBStatistics + Sync + Send + Clone + 'static,
{
    let registry = prometheus::default_registry();

    // Add the DBCollector to the registry
    let db_collector = crate::db::DBCollector::new(db_directory);
    registry.register(Box::new(db_collector))?;

    // Create an configure HTTP server
    let app = Router::new()
        .route("/metrics", get(collect_prometheus_metrics))
        .route("/stats/db", get(collect_db_metrics::<DB>))
        .with_state(db);
    let server = axum::Server::from_tcp(prometheus_listener)?.serve(app.into_make_service());

    // Wait for server to exit
    Ok(server.await?)
}

async fn collect_prometheus_metrics() -> impl IntoResponse {
    let registry = prometheus::default_registry();
    let metric_families = registry.gather();
    let mut metrics = vec![];

    let encoder = TextEncoder::new();
    encoder
        .encode(&metric_families, &mut metrics)
        .expect("Encoding Prometheus metrics must succeed.");

    for (_name, registry) in REGISTRIES_EXT.read().await.iter() {
        if let Err(e) = prometheus_client::encoding::text::encode(&mut metrics, registry) {
            warn!("{e}");
        }
    }

    (
        StatusCode::OK,
        [("content-type", "text/plain; charset=utf-8")],
        metrics,
    )
}

#[allow(clippy::unused_async)]
async fn collect_db_metrics<DB>(
    axum::extract::State(db): axum::extract::State<DB>,
) -> impl IntoResponse
where
    DB: DBStatistics + Sync + Send + Clone + 'static,
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
