// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use lazy_static::lazy_static;
use prometheus::core::{AtomicU64, GenericCounterVec, Opts};

lazy_static! {
    pub static ref LRU_CACHE_TOTAL: Box<GenericCounterVec<AtomicU64>> = {
        let lru_cache_total = Box::new(
            GenericCounterVec::<AtomicU64>::new(
                Opts::new("lru_cache_total", "Stats of lru caches"),
                &[labels::LRU_CACHE_KIND],
            )
            .expect("Defining the lru_cache_total metric must succeed"),
        );
        prometheus::default_registry()
            .register(lru_cache_total.clone())
            .expect(
                "Registering the lru_cache_total metric with the metrics registry must succeed",
            );
        lru_cache_total
    };
}

pub mod labels {
    pub const LRU_CACHE_KIND: &str = "lru_cache_kind";
}

pub mod values {
    // lru_cache_total
    pub const TIPSET_LRU_HIT: &str = "tipset_lru_hit";
    pub const TIPSET_LRU_MISS: &str = "tipset_lru_miss";
    pub const SKIP_LRU_HIT: &str = "skip_lru_hit";
    pub const SKIP_LRU_MISS: &str = "skip_lru_miss";
}
