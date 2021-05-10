// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use log::info;
use prometheus::{Encoder, Registry, TextEncoder};
use thiserror::Error;

use std::net::SocketAddr;

#[derive(Debug, Error)]
pub enum Error {
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

pub async fn init_prometheus(prometheus_addr: SocketAddr, registry: Registry) -> Result<(), Error> {
    info!("Prometheus server started at {}", prometheus_addr);

    // Create an configure HTTP server
    let mut server = tide::with_state(registry);
    server.at("/metrics").get(collect_metrics);

    // Wait for server to exit
    server.listen(prometheus_addr).await.map_err(Error::Io)
}

async fn collect_metrics(req: tide::Request<Registry>) -> tide::Result {
    let metric_families = req.state().gather();
    let mut metrics = vec![];

    let encoder = TextEncoder::new();
    encoder
        .encode(&metric_families, &mut metrics)
        .expect("Encoding Prometheus metrics must succeed.");
    Ok(tide::Response::builder(tide::StatusCode::Ok)
        .body(metrics)
        .build())
}
