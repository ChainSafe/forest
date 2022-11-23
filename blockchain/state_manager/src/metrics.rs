// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use lazy_static::lazy_static;
use prometheus::{core::Opts, Histogram, HistogramOpts};

lazy_static! {
    pub static ref APPLY_BLOCKS_TIME: Box<Histogram> =
        {
            let apply_blocks_time = Box::new(
                Histogram::with_opts(HistogramOpts {
                    common_opts: Opts::new(
                        "apply_blocks_time",
                        "Duration of routine which applied blocks",
                    ),
                    buckets: vec![],
                })
                .expect("Defining the apply_blocks_time metric must succeed"),
            );
            prometheus::default_registry().register(apply_blocks_time.clone()).expect(
            "Registering the apply_blocks_time metric with the metrics registry must succeed",
        );
            apply_blocks_time
        };
}
