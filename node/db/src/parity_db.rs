// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use crate::parity_db_config::ParityDbConfig;
use crate::utils::bitswap_missing_blocks;
use crate::{DBStatistics, Store};
use anyhow::anyhow;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use libp2p_bitswap::BitswapStore;
use log::warn;
use parity_db::{CompressionType, Db, Options};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct ParityDb {
    pub db: Arc<parity_db::Db>,
    statistics_enabled: bool,
}

/// Converts string to a compression `ParityDb` variant.
fn compression_type_from_str(s: &str) -> anyhow::Result<CompressionType> {
    match s.to_lowercase().as_str() {
        "none" => Ok(CompressionType::NoCompression),
        "lz4" => Ok(CompressionType::Lz4),
        "snappy" => Ok(CompressionType::Snappy),
        _ => Err(anyhow!("invalid compression option")),
    }
}

impl ParityDb {
    fn to_options(path: PathBuf, config: &ParityDbConfig) -> anyhow::Result<Options> {
        const COLUMNS: usize = 1;
        let compression = compression_type_from_str(&config.compression_type)?;
        Ok(Options {
            path,
            sync_wal: true,
            sync_data: true,
            stats: config.enable_statistics,
            salt: None,
            columns: (0..COLUMNS)
                .map(|_| parity_db::ColumnOptions {
                    compression,
                    ..Default::default()
                })
                .collect(),
            compression_threshold: HashMap::new(),
        })
    }

    pub fn open(path: PathBuf, config: &ParityDbConfig) -> anyhow::Result<Self> {
        let opts = Self::to_options(path, config)?;
        Ok(Self {
            db: Arc::new(Db::open_or_create(&opts)?),
            statistics_enabled: opts.stats,
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

impl DBStatistics for ParityDb {
    fn get_statistics(&self) -> Option<String> {
        if !self.statistics_enabled {
            return None;
        }

        let mut buf = Vec::new();
        if let Err(err) = self.db.write_stats_text(&mut buf, None) {
            warn!("Unable to write database statistics: {err}");
            return None;
        }

        match String::from_utf8(buf) {
            Ok(stats) => Some(stats),
            Err(e) => {
                warn!("Malformed statistics: {e}");
                None
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use parity_db::CompressionType;

    #[test]
    fn compression_type_from_str_test() {
        let test_cases = [
            ("lz4", Ok(CompressionType::Lz4)),
            ("SNAPPY", Ok(CompressionType::Snappy)),
            ("none", Ok(CompressionType::NoCompression)),
            ("cthulhu", Err(anyhow!("some error message"))),
        ];
        for (input, expected) in test_cases {
            let actual = compression_type_from_str(input);
            if let Ok(compression) = actual {
                assert_eq!(expected.unwrap(), compression);
            } else {
                assert!(expected.is_err());
            }
        }
    }
}
