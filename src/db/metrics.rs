// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use once_cell::sync::Lazy;
use prometheus::{Histogram, HistogramOpts};

pub static BLOCK_SIZE_BYTES: Lazy<Box<Histogram>> = Lazy::new(|| {
    let block_size = Box::new(
        Histogram::with_opts(
            // No way to set quantile 95 tho
            HistogramOpts::new("block_size", "Histogram of block size").buckets(vec![
                32., 64., 128., 256., 512., 1024., 2048., 4096., 8192., 16384., 32768., 65536.,
            ]),
        )
        .unwrap(),
    );
    prometheus::default_registry()
        .register(block_size.clone())
        .expect("Registering the block_size_bytes metric with the metrics registry must succeed");
    block_size
});
