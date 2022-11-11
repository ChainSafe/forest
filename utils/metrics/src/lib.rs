// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod db;

use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};
use prometheus::{Encoder, TextEncoder};
use std::net::TcpListener;

pub async fn init_prometheus(
    prometheus_listener: TcpListener,
    db_directory: String,
) -> anyhow::Result<()> {
    let registry = prometheus::default_registry();

    // Add the DBCollector to the registry
    let db_collector = crate::db::DBCollector::new(db_directory);
    registry.register(Box::new(db_collector))?;

    // Create an configure HTTP server
    let app = Router::new().route("/metrics", get(collect_metrics));
    let server = axum::Server::from_tcp(prometheus_listener)?.serve(app.into_make_service());

    // Wait for server to exit
    Ok(server.await?)
}

async fn collect_metrics() -> impl IntoResponse {
    let registry = prometheus::default_registry();
    let metric_families = registry.gather();
    let mut metrics = vec![];

    let encoder = TextEncoder::new();
    encoder
        .encode(&metric_families, &mut metrics)
        .expect("Encoding Prometheus metrics must succeed.");

    (
        StatusCode::OK,
        [("content-type", "text/plain; charset=utf-8")],
        metrics,
    )
}
