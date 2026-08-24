// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{counter::Counter, family::Family, histogram::Histogram},
};
use std::sync::LazyLock;

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet, derive_more::Constructor)]
pub struct DrandSourceLabel {
    pub source: &'static str,
}

// drand_entry_source_total
pub const CACHE: DrandSourceLabel = DrandSourceLabel::new("cache");
pub const HTTP: DrandSourceLabel = DrandSourceLabel::new("http");
pub const HTTP_ERROR: DrandSourceLabel = DrandSourceLabel::new("http_error");

/// Counts every round served by [`crate::beacon::Beacon::entry`], labelled by where it
/// came from.
pub static DRAND_ENTRY_SOURCE_TOTAL: LazyLock<Family<DrandSourceLabel, Counter>> =
    LazyLock::new(|| {
        let metric = Family::default();
        crate::metrics::default_registry().register(
            "drand_entry_source_total",
            "Total number of drand rounds served, by source",
            metric.clone(),
        );
        metric
    });

/// Wall-clock duration of a drand HTTP fetch, covering the whole retry chain across
/// every configured server rather than a single attempt.
pub static DRAND_HTTP_FETCH_TIME: LazyLock<Histogram> = LazyLock::new(|| {
    let metric = crate::metrics::default_histogram();
    crate::metrics::default_registry().register(
        "drand_http_fetch_time",
        "Duration of a drand HTTP round fetch, including retries across servers",
        metric.clone(),
    );
    metric
});
