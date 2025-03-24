// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use once_cell::sync::Lazy;
use prometheus_client::metrics::histogram::Histogram;

pub static CONSENSUS_BLOCK_VALIDATION_TIME: Lazy<Histogram> = Lazy::new(|| {
    let metric = crate::metrics::default_histogram();
    crate::metrics::default_registry().register(
        "cns_block_validation_time",
        "Duration of routine which validate blocks in fil_cns",
        metric.clone(),
    );
    metric
});
