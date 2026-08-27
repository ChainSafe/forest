// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use prometheus_client::metrics::counter::Counter;
use std::sync::LazyLock;

/// Counts the rounds [`crate::beacon::Beacon::entry`] had to fetch over HTTP, that is,
/// the ones it could not serve from the in-memory cache.
pub static DRAND_HTTP_FETCH_TOTAL: LazyLock<Counter> = LazyLock::new(|| {
    let metric = Counter::default();
    crate::metrics::default_registry().register(
        "drand_http_fetch_total",
        "Total number of drand rounds fetched over HTTP",
        metric.clone(),
    );
    metric
});
