// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use once_cell::sync::Lazy;
use prometheus_client::metrics::gauge::Gauge;

pub static MPOOL_MESSAGE_TOTAL: Lazy<Gauge> = Lazy::new(|| {
    let metric = Gauge::default();
    crate::metrics::default_registry().register(
        "mpool_message_total",
        "Total number of messages in the message pool",
        metric.clone(),
    );
    metric
});
