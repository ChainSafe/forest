// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use lazy_static::lazy_static;
use prometheus::core::{AtomicU64, GenericCounterVec, Opts};

lazy_static! {
    pub static ref LRU_CACHE_HIT: Box<GenericCounterVec<AtomicU64>> = {
        let lru_cache_hit = Box::new(
            GenericCounterVec::<AtomicU64>::new(
                Opts::new("lru_cache_hit", "Stats of lru cache hit"),
                &[labels::KIND],
            )
            .expect("Defining the lru_cache_hit metric must succeed"),
        );
        prometheus::default_registry()
            .register(lru_cache_hit.clone())
            .expect("Registering the lru_cache_hit metric with the metrics registry must succeed");
        lru_cache_hit
    };
    pub static ref LRU_CACHE_MISS: Box<GenericCounterVec<AtomicU64>> = {
        let lru_cache_miss = Box::new(
            GenericCounterVec::<AtomicU64>::new(
                Opts::new("lru_cache_miss", "Stats of lru cache miss"),
                &[labels::KIND],
            )
            .expect("Defining the lru_cache_miss metric must succeed"),
        );
        prometheus::default_registry()
            .register(lru_cache_miss.clone())
            .expect("Registering the lru_cache_miss metric with the metrics registry must succeed");
        lru_cache_miss
    };
}

pub mod labels {
    pub const KIND: &str = "kind";
}

pub mod values {
    /// `TipsetCache`.
    pub const TIPSET: &str = "tipset";
    /// Cache of look-back entries to speed up lookup in `ChainIndex`.
    pub const SKIP: &str = "skip";
}
