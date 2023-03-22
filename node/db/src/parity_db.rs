// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{path::PathBuf, sync::Arc};

use anyhow::anyhow;
use cid::Cid;
use forest_libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};
use fvm_ipld_blockstore::Blockstore;
use log::warn;
use parity_db::{CompressionType, Db, Operation, Options};

use super::errors::Error;
use crate::{parity_db_config::ParityDbConfig, DBStatistics, Store};

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
                    // btree_index: true,
                    ..Default::default()
                })
                .collect(),
            compression_threshold: [(0, 128)].into_iter().collect(),
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
        let tx = [(0, key.as_ref(), Some(value.as_ref().to_vec()))];
        self.db.commit(tx).map_err(Error::from)
    }

    /// [parity_db::Db::commit] API is doing extra allocations on keys,
    /// See <https://docs.rs/crate/parity-db/0.4.3/source/src/db.rs>
    fn bulk_write(
        &self,
        values: impl IntoIterator<Item = (impl Into<Vec<u8>>, impl Into<Vec<u8>>)>,
    ) -> Result<(), Error> {
        let tx = values
            .into_iter()
            .map(|(k, v)| (0, Operation::Set(k.into(), v.into())));
        self.db.commit_changes(tx).map_err(Error::from)
        // <https://docs.rs/crate/parity-db/0.4.3/source/src/db.rs>
        // ```
        // fn commit<I, K>(&self, tx: I) -> Result<()>
        // where
        //     I: IntoIterator<Item = (ColId, K, Option<Value>)>,
        //     K: AsRef<[u8]>,
        // {
        //     self.commit_changes(tx.into_iter().map(|(c, k, v)| {
        //         (
        //             c,
        //             match v {
        //                 Some(v) => Operation::Set(k.as_ref().to_vec(), v),
        //                 None => Operation::Dereference(k.as_ref().to_vec()),
        //             },
        //         )
        //     }))
        // }
        // ```
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
            .map(|(k, v)| (k.to_bytes(), v.as_ref().to_vec()));
        self.bulk_write(values).map_err(|e| e.into())
    }
}

impl BitswapStoreRead for ParityDb {
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
        Ok(self.exists(cid.to_bytes())?)
    }

    fn get(&self, cid: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        Blockstore::get(self, cid)
    }
}

impl BitswapStoreReadWrite for ParityDb {
    /// `fvm_ipld_encoding::DAG_CBOR(0x71)` is covered by
    /// [`libipld::DefaultParams`] under feature `dag-cbor`
    type Params = libipld::DefaultParams;

    fn insert(&self, block: &libipld::Block<Self::Params>) -> anyhow::Result<()> {
        self.put_keyed(block.cid(), block.data())
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
    use parity_db::CompressionType;

    use super::*;

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
