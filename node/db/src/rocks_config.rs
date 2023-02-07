// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use num_cpus;
use serde::{Deserialize, Serialize};

/// `RocksDB` configuration exposed in Forest.
/// Only subset of possible options is implemented, add missing ones when
/// needed. For description of different options please refer to the `rocksdb`
/// crate documentation. <https://docs.rs/rocksdb/latest/rocksdb/>
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RocksDbConfig {
    pub create_if_missing: bool,
    pub parallelism: i32,
    /// This is the `memtable` size in bytes.
    pub write_buffer_size: usize,
    pub max_open_files: i32,
    pub max_background_jobs: Option<i32>,
    pub compaction_style: String,
    pub compression_type: String,
    pub enable_statistics: bool,
    pub stats_dump_period_sec: u32,
    pub log_level: String,
    pub optimize_filters_for_hits: bool,
    pub optimize_for_point_lookup: i32,
}

impl Default for RocksDbConfig {
    fn default() -> Self {
        Self {
            create_if_missing: true,
            parallelism: num_cpus::get() as i32,
            write_buffer_size: 2usize.pow(30), // 1 GiB
            max_open_files: -1,
            max_background_jobs: None,
            compaction_style: "none".into(),
            compression_type: "lz4".into(),
            enable_statistics: false,
            stats_dump_period_sec: 600,
            log_level: "warn".into(),
            optimize_filters_for_hits: true,
            optimize_for_point_lookup: 8,
        }
    }
}
