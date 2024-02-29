// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use once_cell::sync::Lazy;
use prometheus_client::metrics::{counter::Counter, gauge::Gauge};

pub static PEER_FAILURE_TOTAL: Lazy<Counter> = Lazy::new(|| {
    let metric = Counter::default();
    crate::metrics::default_registry().register(
        "peer_failure_total",
        "Total number of failed peer requests",
        metric.clone(),
    );
    metric
});

pub static FULL_PEERS: Lazy<Gauge> = Lazy::new(|| {
    let metric = Gauge::default();
    crate::metrics::default_registry().register(
        "full_peers",
        "Number of healthy peers recognized by the node",
        metric.clone(),
    );
    metric
});

pub static BAD_PEERS: Lazy<Gauge> = Lazy::new(|| {
    let metric = Gauge::default();
    crate::metrics::default_registry().register(
        "bad_peers",
        "Number of bad peers recognized by the node",
        metric.clone(),
    );
    metric
});
