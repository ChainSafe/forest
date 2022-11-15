// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use lazy_static::lazy_static;
use prometheus::core::{AtomicU64, GenericCounterVec, Opts};

lazy_static! {
    pub static ref LRU_CACHE_HIT: Box<GenericCounterVec<AtomicU64>> = {
        let lru_cache_hit = Box::new(
            GenericCounterVec::<AtomicU64>::new(
                Opts::new("lru_cache_hit", "Stats of lru cache hit"),
                &[labels::LRU_CACHE_HIT_KIND],
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
                &[labels::LRU_CACHE_MISS_KIND],
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
    pub const LRU_CACHE_HIT_KIND: &str = "lru_cache_hit_kind";
    pub const LRU_CACHE_MISS_KIND: &str = "lru_cache_miss_kind";
}

pub mod values {
    /// Cache hit of `TipsetCache`.
    pub const TIPSET_LRU_HIT: &str = "tipset_lru_hit";
    /// Cache miss of `TipsetCache`.
    pub const TIPSET_LRU_MISS: &str = "tipset_lru_miss";
    /// Cache hit of look-back entries to speed up lookup in `ChainIndex`.
    pub const SKIP_LRU_HIT: &str = "skip_lru_hit";
    /// Cache miss of look-back entries to speed up lookup in `ChainIndex`.
    pub const SKIP_LRU_MISS: &str = "skip_lru_miss";
}
