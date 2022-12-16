// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use crate::utils::bitswap_missing_blocks;
use crate::{DBStatistics, Store};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use libp2p_bitswap::BitswapStore;
use parity_db::Db;
use parity_db::Options;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct ParityDb {
    pub db: Arc<parity_db::Db>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ParityDbConfig {
    pub columns: u8,
}

impl Default for ParityDbConfig {
    fn default() -> Self {
        Self { columns: 1 }
    }
}

impl ParityDb {
    fn to_options(path: &Path, config: &ParityDbConfig) -> Options {
        Options {
            path: path.to_path_buf(),
            sync_wal: true,
            sync_data: true,
            stats: true,
            salt: None,
            columns: (0..config.columns)
                .map(|_| parity_db::ColumnOptions {
                    compression: parity_db::CompressionType::Lz4,
                    ..Default::default()
                })
                .collect(),
            compression_threshold: HashMap::new(),
        }
    }

    pub fn open(path: &Path, config: &ParityDbConfig) -> Result<Self, Error> {
        let db_opts = Self::to_options(path, config);
        Ok(Self {
            db: Arc::new(Db::open_or_create(&db_opts)?),
        })
    }
}

impl Store for ParityDb {
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        self.db.get(0, key.as_ref()).map_err(Error::from)
    }

    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        let tx = [(0, key.as_ref(), Some(value.as_ref().to_owned()))];
        self.db.commit(tx).map_err(Error::from)
    }

    fn bulk_write<K, V>(&self, values: &[(K, V)]) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        let tx = values
            .iter()
            .map(|(k, v)| (0, k.as_ref(), Some(v.as_ref().to_owned())))
            .collect::<Vec<_>>();

        self.db.commit(tx).map_err(Error::from)
    }

    fn delete<K>(&self, key: K) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
    {
        let tx = [(0, key.as_ref(), None)];
        self.db.commit(tx).map_err(Error::from)
    }

    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        self.db
            .get_size(0, key.as_ref())
            .map(|size| size.is_some())
            .map_err(Error::from)
    }
}

impl Blockstore for ParityDb {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        self.read(k.to_bytes()).map_err(|e| e.into())
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
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
        self.bulk_write(&values).map_err(|e| e.into())
    }
}

impl BitswapStore for ParityDb {
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

impl DBStatistics for ParityDb {}
