// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use anyhow::anyhow;
use num_cpus;
use rocksdb::{DBCompactionStyle, DBCompressionType};
use serde::{Deserialize, Serialize};

/// RocksDB configuration exposed in Forest.
/// Only subset of possible options is implemented, add missing ones when needed.
/// For description of different options please refer to the `rocksdb` crate documentation.
/// <https://docs.rs/rocksdb/latest/rocksdb/>
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RocksDbConfig {
    pub create_if_missing: bool,
    pub parallelism: i32,
    pub write_buffer_size: usize,
    pub max_open_files: i32,
    pub max_background_jobs: Option<i32>,
    pub compression_type: Option<String>,
    pub compaction_style: Option<String>,
    pub enable_statistics: bool,
}

impl Default for RocksDbConfig {
    fn default() -> Self {
        Self {
            create_if_missing: true,
            parallelism: num_cpus::get() as i32,
            write_buffer_size: 256 * 1024 * 1024,
            max_open_files: 1024,
            max_background_jobs: None,
            compaction_style: None,
            compression_type: Some("lz4".into()),
            enable_statistics: false,
        }
    }
}

/// Converts string to a compaction style RocksDB variant.
pub(crate) fn compaction_style_from_str(s: &str) -> anyhow::Result<DBCompactionStyle> {
    match s.to_lowercase().as_str() {
        "level" => Ok(DBCompactionStyle::Level),
        "universal" => Ok(DBCompactionStyle::Universal),
        "fifo" => Ok(DBCompactionStyle::Fifo),
        _ => Err(anyhow!("invalid compaction option")),
    }
}

/// Converts string to a compression type RocksDB variant.
pub(crate) fn compression_type_from_str(s: &str) -> anyhow::Result<DBCompressionType> {
    match s.to_lowercase().as_str() {
        "bz2" => Ok(DBCompressionType::Bz2),
        "lz4" => Ok(DBCompressionType::Lz4),
        "lz4hc" => Ok(DBCompressionType::Lz4hc),
        "snappy" => Ok(DBCompressionType::Snappy),
        "zlib" => Ok(DBCompressionType::Zlib),
        "zstd" => Ok(DBCompressionType::Zstd),
        "none" => Ok(DBCompressionType::None),
        _ => Err(anyhow!("invalid compression option")),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rocksdb::DBCompactionStyle;

    #[test]
    fn compaction_style_from_str_test() {
        let test_cases = vec![
            ("Level", Ok(DBCompactionStyle::Level)),
            ("UNIVERSAL", Ok(DBCompactionStyle::Universal)),
            ("fifo", Ok(DBCompactionStyle::Fifo)),
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
            ("bz2", Ok(DBCompressionType::Bz2)),
            ("lz4", Ok(DBCompressionType::Lz4)),
            ("lz4HC", Ok(DBCompressionType::Lz4hc)),
            ("SNAPPY", Ok(DBCompressionType::Snappy)),
            ("zlib", Ok(DBCompressionType::Zlib)),
            ("ZSTD", Ok(DBCompressionType::Zstd)),
            ("none", Ok(DBCompressionType::None)),
            ("cthulhu", Err(anyhow!("some error message"))),
        ];
        for (input, expected) in test_cases {
            let actual = compression_type_from_str(input);
            if let Ok(compression_type) = actual {
                assert_eq!(expected.unwrap(), compression_type);
            } else {
                assert!(expected.is_err());
            }
        }
    }
}
