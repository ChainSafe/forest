// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use super::Store;
use crate::{
    metrics,
    rocks_config::{
        compaction_style_from_str, compression_type_from_str, log_level_from_str, RocksDbConfig,
    },
    utils::bitswap_missing_blocks,
};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use libp2p_bitswap::BitswapStore;
pub use rocksdb::{
    BlockBasedOptions, Cache, CompactOptions, DBCompressionType, DataBlockIndexType, Options,
    WriteBatch, DB,
};
use std::{path::Path, sync::Arc};

/// `RocksDB` instance this satisfies the [Store] interface.
#[derive(Clone)]
pub struct RocksDb {
    pub db: Arc<DB>,
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
        db_opts.set_compression_type(compression_type_from_str(&config.compression_type).unwrap());
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
        })
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
        Ok(self.db.put(key, value)?)
    }

    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(self.db.delete(key)?)
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

    fn bulk_write<K, V>(&self, values: &[(K, V)]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        let mut batch = WriteBatch::default();
        for (k, v) in values {
            batch.put(k, v);
        }
        Ok(self.db.write(batch)?)
    }
}

impl Blockstore for RocksDb {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        self.read(k.to_bytes()).map_err(|e| e.into())
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        metrics::BLOCK_SIZE_BYTES.observe(block.len() as f64);
        self.write(k.to_bytes(), block).map_err(|e| e.into())
    }

    fn put_many_keyed<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (Cid, D)>,
    {
        let values = blocks
            .into_iter()
            .map(|(k, v)| (k.to_bytes(), v))
            .collect::<Vec<_>>();
        for (_k, v) in &values {
            metrics::BLOCK_SIZE_BYTES.observe(v.as_ref().len() as f64);
        }
        self.bulk_write(&values).map_err(|e| e.into())
    }
}

impl BitswapStore for RocksDb {
    /// `fvm_ipld_encoding::DAG_CBOR(0x71)` is covered by [`libipld::DefaultParams`]
    /// under feature `dag-cbor`
    type Params = libipld::DefaultParams;

    fn contains(&mut self, cid: &Cid) -> anyhow::Result<bool> {
        Ok(self.exists(cid.to_bytes())?)
    }

    fn get(&mut self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        Blockstore::get(self, cid)
    }

    fn insert(&mut self, block: &libipld::Block<Self::Params>) -> anyhow::Result<()> {
        self.put_keyed(block.cid(), block.data())
    }

    fn missing_blocks(&mut self, cid: &Cid) -> anyhow::Result<Vec<Cid>> {
        bitswap_missing_blocks::<_, Self::Params>(self, cid)
    }
}
