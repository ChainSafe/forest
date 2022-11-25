// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use anyhow::anyhow;
use num_cpus;
use rocksdb::{DBCompactionStyle, DBCompressionType, LogLevel};
use serde::{Deserialize, Serialize};

/// `RocksDB` configuration exposed in Forest.
/// Only subset of possible options is implemented, add missing ones when needed.
/// For description of different options please refer to the `rocksdb` crate documentation.
/// <https://docs.rs/rocksdb/latest/rocksdb/>
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

/// Converts string to a compaction style `RocksDB` variant.
pub(crate) fn compaction_style_from_str(s: &str) -> anyhow::Result<Option<DBCompactionStyle>> {
    match s.to_lowercase().as_str() {
        "level" => Ok(Some(DBCompactionStyle::Level)),
        "universal" => Ok(Some(DBCompactionStyle::Universal)),
        "fifo" => Ok(Some(DBCompactionStyle::Fifo)),
        "none" => Ok(None),
        _ => Err(anyhow!("invalid compaction option")),
    }
}

/// Converts string to a compression type `RocksDB` variant.
pub(crate) fn compression_type_from_str(s: &str) -> anyhow::Result<DBCompressionType> {
    let valid_options = [
        #[cfg(feature = "bzip2")]
        "bz2",
        #[cfg(feature = "lz4")]
        "lz4",
        #[cfg(feature = "lz4")]
        "lz4hc",
        #[cfg(feature = "snappy")]
        "snappy",
        #[cfg(feature = "zlib")]
        "zlib",
        #[cfg(feature = "zstd")]
        "zstd",
        "none",
    ];
    match s.to_lowercase().as_str() {
        #[cfg(feature = "bzip2")]
        "bz2" => Ok(DBCompressionType::Bz2),
        #[cfg(feature = "lz4")]
        "lz4" => Ok(DBCompressionType::Lz4),
        #[cfg(feature = "lz4")]
        "lz4hc" => Ok(DBCompressionType::Lz4hc),
        #[cfg(feature = "snappy")]
        "snappy" => Ok(DBCompressionType::Snappy),
        #[cfg(feature = "zlib")]
        "zlib" => Ok(DBCompressionType::Zlib),
        #[cfg(feature = "zstd")]
        "zstd" => Ok(DBCompressionType::Zstd),
        "none" => Ok(DBCompressionType::None),
        opt => Err(anyhow!(
            "invalid compression option: {opt}, valid options: {}",
            valid_options.join(",")
        )),
    }
}

/// Converts string to a log level `RocksDB` variant.
pub(crate) fn log_level_from_str(s: &str) -> anyhow::Result<LogLevel> {
    match s.to_lowercase().as_str() {
        "debug" => Ok(LogLevel::Debug),
        "warn" => Ok(LogLevel::Warn),
        "error" => Ok(LogLevel::Error),
        "fatal" => Ok(LogLevel::Fatal),
        "header" => Ok(LogLevel::Header),
        _ => Err(anyhow!("invalid log level option")),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rocksdb::DBCompactionStyle;

    #[test]
    fn compaction_style_from_str_test() {
        let test_cases = vec![
            ("Level", Ok(Some(DBCompactionStyle::Level))),
            ("UNIVERSAL", Ok(Some(DBCompactionStyle::Universal))),
            ("fifo", Ok(Some(DBCompactionStyle::Fifo))),
            ("none", Ok(None)),
            ("cthulhu", Err(anyhow!("some error message"))),
        ];
        for (input, expected) in test_cases {
            let actual = compaction_style_from_str(input);
            if let Ok(compaction_style) = actual {
                assert_eq!(expected.unwrap(), compaction_style);
            } else {
                assert!(expected.is_err());
            }
        }
    }

    #[test]
    fn compression_style_from_str_test() {
        let test_cases = vec![
            #[cfg(feature = "bzip2")]
            ("bz2", Ok(DBCompressionType::Bz2)),
            #[cfg(feature = "lz4")]
            ("lz4", Ok(DBCompressionType::Lz4)),
            #[cfg(feature = "lz4")]
            ("lz4HC", Ok(DBCompressionType::Lz4hc)),
            #[cfg(feature = "snappy")]
            ("SNAPPY", Ok(DBCompressionType::Snappy)),
            #[cfg(feature = "zlib")]
            ("zlib", Ok(DBCompressionType::Zlib)),
            #[cfg(feature = "zstd")]
            ("ZSTD", Ok(DBCompressionType::Zstd)),
            ("none", Ok(DBCompressionType::None)),
            ("cthulhu", Err(anyhow!("some error message"))),
        ];
        for (input, expected) in test_cases {
            let actual = compression_type_from_str(input);
            if let Ok(compression_type) = actual {
                assert_eq!(expected.unwrap(), compression_type);
                let dir = tempfile::tempdir().unwrap();
                let mut opt = rocksdb::Options::default();
                opt.create_if_missing(true);
                opt.set_compression_type(compression_type);
                rocksdb::DB::open(&opt, dir.path()).unwrap();
            } else {
                assert!(expected.is_err());
            }
        }
    }
}
