// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod db;

use log::info;
use prometheus::{Encoder, TextEncoder};
use thiserror::Error;

use std::net::SocketAddr;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Prometheus error: {0}")]
    Prometheus(prometheus::Error),
    /// Tide internal error.
    #[error("Tide error: {0}")]
    Tide(tide::Error),
    /// I/O error.
    #[error("IO error: {0}")]
    Io(std::io::Error),
    /// Prometheus port is already in use.
    #[error("Prometheus port {0} is already in use.")]
    PortInUse(SocketAddr),
}

pub async fn init_prometheus(
    prometheus_addr: SocketAddr,
    db_directory: String,
) -> Result<(), Error> {
    info!("Prometheus server started at {}", prometheus_addr);

    let registry = prometheus::default_registry();

    // Add the DBCollector to the registry
    let db_collector = crate::db::DBCollector::new(db_directory);
    registry
        .register(Box::new(db_collector))
        .map_err(Error::Prometheus)?;

    // Create an configure HTTP server
    let mut server = tide::with_state(());
    server.at("/metrics").get(collect_metrics);

    // Wait for server to exit
    server.listen(prometheus_addr).await.map_err(Error::Io)
}

async fn collect_metrics(_req: tide::Request<()>) -> tide::Result {
    let registry = prometheus::default_registry();
    let metric_families = registry.gather();
    let mut metrics = vec![];

    let encoder = TextEncoder::new();
    encoder
        .encode(&metric_families, &mut metrics)
        .expect("Encoding Prometheus metrics must succeed.");
    Ok(tide::Response::builder(tide::StatusCode::Ok)
        .content_type("text/plain; version=0.0.4; charset=utf-8")
        .body(metrics)
        .build())
}
