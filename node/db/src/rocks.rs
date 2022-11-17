// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use super::Store;
use crate::{metrics, rocks_config::RocksDbConfig};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use rocksdb::WriteOptions;
pub use rocksdb::{
    BlockBasedOptions, Cache, CompactOptions, DBCompressionType, DataBlockIndexType, Options,
    WriteBatch, DB,
};
use std::{path::Path, sync::Arc};

pub mod columns {
    pub const BLOCK_VALIDATION_COLUMN: &str = "block_validation";
    pub const CHAIN_INFO_COLUMN: &str = "chain_info";
}

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
    pub fn open<P>(path: P, config: &RocksDbConfig) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let db_opts = config.to_options();
        let mut db = DB::open(&db_opts, path)?;
        for col in [columns::BLOCK_VALIDATION_COLUMN, columns::CHAIN_INFO_COLUMN] {
            if db.cf_handle(col).is_none() {
                db.create_cf(col, &db_opts)?;
            }
        }
        Ok(Self { db: Arc::new(db) })
    }

    pub fn from_raw_db(db: DB) -> Self {
        Self { db: Arc::new(db) }
    }
}

impl Store for RocksDb {
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        self.db.get(key).map_err(Error::from)
    }

    fn read_column<K>(&self, key: K, column: &str) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        let cf = self
            .db
            .cf_handle(column)
            .ok_or(Error::GetColumnFamilyHandle)?;
        self.db.get_cf(&cf, key).map_err(Error::from)
    }

    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        let mut opt = WriteOptions::default();
        opt.disable_wal(true);
        Ok(self.db.put_opt(key, value, &opt)?)
    }

    fn write_column<K, V>(&self, key: K, value: V, column: &str) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        let cf = self
            .db
            .cf_handle(column)
            .ok_or(Error::GetColumnFamilyHandle)?;
        let mut opt = WriteOptions::default();
        opt.disable_wal(true);
        Ok(self.db.put_cf_opt(&cf, key, value, &opt)?)
    }

    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(self.db.delete(key)?)
    }

    fn delete_column<K>(&self, key: K, column: &str) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        let cf = self
            .db
            .cf_handle(column)
            .ok_or(Error::GetColumnFamilyHandle)?;
        Ok(self.db.delete_cf(&cf, key)?)
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

    fn exists_column<K>(&self, key: K, column: &str) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        let cf = self
            .db
            .cf_handle(column)
            .ok_or(Error::GetColumnFamilyHandle)?;
        self.db
            .get_pinned_cf(&cf, key)
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

    fn bulk_write_column<K, V>(&self, values: &[(K, V)], column: &str) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        let cf = self
            .db
            .cf_handle(column)
            .ok_or(Error::GetColumnFamilyHandle)?;
        let mut batch = WriteBatch::default();
        for (k, v) in values {
            batch.put_cf(&cf, k, v);
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
