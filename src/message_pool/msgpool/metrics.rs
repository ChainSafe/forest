// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use prometheus_client::metrics::gauge::Gauge;
use std::sync::LazyLock;

pub static MPOOL_MESSAGE_TOTAL: LazyLock<Gauge> = LazyLock::new(|| {
    let metric = Gauge::default();
    crate::metrics::default_registry().register(
        "mpool_message_total",
        "Total number of messages in the message pool",
        metric.clone(),
    );
    metric
});
