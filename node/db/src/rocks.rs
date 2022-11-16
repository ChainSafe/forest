// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use super::Store;
use crate::{metrics, rocks_config::RocksDbConfig};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
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
    pub fn open<P>(path: P, config: &RocksDbConfig) -> Result<Self, Error>
    where
        P: AsRef<Path>,
    {
        let db_opts = config.to_options();
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
        let key = key.as_ref();
        // If the return value of 'key_may_exist' is false, the key definitely does not exist in the db
        if !self.db.key_may_exist(key) {
            Ok(None)
        } else {
            self.db.get(key).map_err(Error::from)
        }
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
        let key = key.as_ref();
        // If the return value of 'key_may_exist' is false, the key definitely does not exist in the db
        Ok(self.db.key_may_exist(key)
            && self
                .db
                .get_pinned(key)
                .map(|v| v.is_some())
                .map_err(Error::from)?)
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
