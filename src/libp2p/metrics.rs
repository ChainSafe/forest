// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use prometheus_client::metrics::{counter::Counter, gauge::Gauge};
use std::sync::LazyLock;

pub static PEER_FAILURE_TOTAL: LazyLock<Counter> = LazyLock::new(|| {
    let metric = Counter::default();
    crate::metrics::default_registry().register(
        "peer_failure_total",
        "Total number of failed peer requests",
        metric.clone(),
    );
    metric
});

pub static FULL_PEERS: LazyLock<Gauge> = LazyLock::new(|| {
    let metric = Gauge::default();
    crate::metrics::default_registry().register(
        "full_peers",
        "Number of healthy peers recognized by the node",
        metric.clone(),
    );
    metric
});

pub static BAD_PEERS: LazyLock<Gauge> = LazyLock::new(|| {
    let metric = Gauge::default();
    crate::metrics::default_registry().register(
        "bad_peers",
        "Number of bad peers recognized by the node",
        metric.clone(),
    );
    metric
});
