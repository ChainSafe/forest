// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{path::Path, sync::Arc};

use anyhow::anyhow;
use cid::Cid;
use forest_libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};
use fvm_ipld_blockstore::Blockstore;
use rocksdb::{
    BlockBasedOptions, Cache, DBCompactionStyle, DBCompressionType, DataBlockIndexType, LogLevel,
    Options, WriteBatch, WriteOptions, DB,
};

use super::{errors::Error, Store};
use crate::{metrics, rocks_config::RocksDbConfig, DBStatistics};

lazy_static::lazy_static! {
    static ref WRITE_OPT_NO_WAL: WriteOptions = {
        let mut opt = WriteOptions::default();
        opt.disable_wal(true);
        opt
    };
}

/// Converts string to a compaction style `RocksDB` variant.
fn compaction_style_from_str(s: &str) -> anyhow::Result<Option<DBCompactionStyle>> {
    match s.to_lowercase().as_str() {
        "level" => Ok(Some(DBCompactionStyle::Level)),
        "universal" => Ok(Some(DBCompactionStyle::Universal)),
        "fifo" => Ok(Some(DBCompactionStyle::Fifo)),
        "none" => Ok(None),
        _ => Err(anyhow!("invalid compaction option")),
    }
}

/// Converts string to a log level `RocksDB` variant.
fn log_level_from_str(s: &str) -> anyhow::Result<LogLevel> {
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
    use rocksdb::DBCompactionStyle;

    use super::*;

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
}

/// `RocksDB` instance this satisfies the [Store] interface.
#[derive(Clone)]
pub struct RocksDb {
    pub db: Arc<DB>,
    options: Options,
}

/// `RocksDb` is used as the KV store for Forest
///
/// Usage:
/// ```no_run
/// use forest_db::rocks::RocksDb;
/// use forest_db::rocks_config::RocksDbConfig;
///
/// let mut db = RocksDb::open("test_db", &RocksDbConfig::default()).unwrap();
/// ```
impl RocksDb {
    fn to_options(config: &RocksDbConfig) -> Options {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(config.create_if_missing);
        db_opts.increase_parallelism(config.parallelism);
        db_opts.set_write_buffer_size(config.write_buffer_size);
        db_opts.set_max_open_files(config.max_open_files);

        if let Some(max_background_jobs) = config.max_background_jobs {
            db_opts.set_max_background_jobs(max_background_jobs);
        }
        if let Some(compaction_style) = compaction_style_from_str(&config.compaction_style).unwrap()
        {
            db_opts.set_compaction_style(compaction_style);
            db_opts.set_disable_auto_compactions(false);
        } else {
            db_opts.set_disable_auto_compactions(true);
        }
        db_opts.set_compression_type(DBCompressionType::Lz4);
        if config.enable_statistics {
            db_opts.set_stats_dump_period_sec(config.stats_dump_period_sec);
            db_opts.enable_statistics();
        };
        db_opts.set_log_level(log_level_from_str(&config.log_level).unwrap());
        db_opts.set_optimize_filters_for_hits(config.optimize_filters_for_hits);
        // Comes from https://github.com/facebook/rocksdb/blob/main/options/options.cc#L606
        // Only modified to upgrade format to v5
        if !config.optimize_for_point_lookup.is_negative() {
            let cache_size = config.optimize_for_point_lookup as usize;
            let mut opts = BlockBasedOptions::default();
            opts.set_format_version(5);
            opts.set_data_block_index_type(DataBlockIndexType::BinaryAndHash);
            opts.set_data_block_hash_ratio(0.75);
            opts.set_bloom_filter(10.0, false);
            let cache = Cache::new_lru_cache(cache_size * 1024 * 1024).unwrap();
            opts.set_block_cache(&cache);
            db_opts.set_block_based_table_factory(&opts);
            db_opts.set_memtable_prefix_bloom_ratio(0.02);
            db_opts.set_memtable_whole_key_filtering(true);
        }
        db_opts
    }

    pub fn open<P>(path: P, config: &RocksDbConfig) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let db_opts = Self::to_options(config);
        Ok(Self {
            db: Arc::new(DB::open(&db_opts, path)?),
            options: db_opts,
        })
    }

    pub fn get_statistics(&self) -> Option<String> {
        self.options.get_statistics()
    }
}

impl Store for RocksDb {
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        self.db.get(key).map_err(Error::from)
    }

    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        Ok(self.db.put_opt(key, value, &WRITE_OPT_NO_WAL)?)
    }

    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        self.db
            .get_pinned(key)
            .map(|v| v.is_some())
            .map_err(Error::from)
    }

    fn bulk_write(
        &self,
        values: impl IntoIterator<Item = (impl Into<Vec<u8>>, impl Into<Vec<u8>>)>,
    ) -> Result<(), Error> {
        let mut batch = WriteBatch::default();
        for (k, v) in values {
            batch.put(k.into(), v.into());
        }
        Ok(self.db.write_without_wal(batch)?)
    }

    fn flush(&self) -> Result<(), Error> {
        self.db.flush().map_err(|e| Error::Other(e.to_string()))
    }
}

impl Blockstore for RocksDb {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        self.read(k.to_bytes()).map_err(Into::into)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        metrics::BLOCK_SIZE_BYTES.observe(block.len() as f64);
        self.write(k.to_bytes(), block).map_err(Into::into)
    }

    fn put_many_keyed<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (Cid, D)>,
    {
        let mut batch = WriteBatch::default();
        for (cid, v) in blocks.into_iter() {
            let k = cid.to_bytes();
            let v = v.as_ref();
            metrics::BLOCK_SIZE_BYTES.observe(v.len() as f64);
            batch.put(k, v);
        }
        // This function is used in `fvm_ipld_car::load_car`
        // It reduces time cost of loading mainnet snapshot
        // by ~10% by not writing to WAL(write ahead log).
        Ok(self.db.write_without_wal(batch)?)
    }
}

impl BitswapStoreRead for RocksDb {
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
        Ok(self.exists(cid.to_bytes())?)
    }

    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        Blockstore::get(self, cid)
    }
}

impl BitswapStoreReadWrite for RocksDb {
    /// `fvm_ipld_encoding::DAG_CBOR(0x71)` is covered by
    /// [`libipld::DefaultParams`] under feature `dag-cbor`
    type Params = libipld::DefaultParams;

    fn insert(&self, block: &libipld::Block<Self::Params>) -> anyhow::Result<()> {
        self.put_keyed(block.cid(), block.data())
    }
}

impl DBStatistics for RocksDb {
    fn get_statistics(&self) -> Option<String> {
        self.options.get_statistics()
    }
}
