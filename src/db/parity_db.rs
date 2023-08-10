// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use super::SettingsStore;

use crate::db::{parity_db_config::ParityDbConfig, DBStatistics};
use crate::libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};

use anyhow::{anyhow, Context};
use cid::multihash::Code::Blake2b256;

use cid::Cid;

use fvm_ipld_blockstore::Blockstore;

use fvm_ipld_encoding::DAG_CBOR;

use parity_db::{CompressionType, Db, Operation, Options};
use strum::{Display, EnumIter, FromRepr, IntoEnumIterator};

use tracing::warn;

/// This is specific to Forest's `ParityDb` usage.
/// It is used to determine which column to use for a given entry type.
#[derive(Copy, Clone, Debug, Display, PartialEq, FromRepr, EnumIter)]
#[repr(u8)]
enum DbColumn {
    /// Column for storing IPLD data with `Blake2b256` hash and `DAG_CBOR` codec.
    /// Most entries in the `blockstore` will be stored in this column.
    GraphDagCborBlake2b256,
    /// Column for storing other IPLD data (different codec or hash function).
    /// It allows for key retrieval at the cost of degraded performance. Given that
    /// there will be a small number of entries in this column, the performance
    /// degradation is negligible.
    GraphFull,
    /// Column for storing Forest-specific settings.
    Settings,
}

impl DbColumn {
    fn create_column_options(compression: CompressionType) -> Vec<parity_db::ColumnOptions> {
        DbColumn::iter()
            .map(|col| {
                match col {
                    DbColumn::GraphDagCborBlake2b256 => parity_db::ColumnOptions {
                        preimage: true,
                        compression,
                        ..Default::default()
                    },
                    DbColumn::GraphFull => parity_db::ColumnOptions {
                        preimage: true,
                        // This is needed for key retrieval.
                        btree_index: true,
                        compression,
                        ..Default::default()
                    },
                    DbColumn::Settings => parity_db::ColumnOptions {
                        // explicitly disable preimage for settings column
                        // othewise we are not able to overwrite entries
                        preimage: false,
                        // This is needed for key retrieval.
                        btree_index: true,
                        compression,
                        ..Default::default()
                    },
                }
            })
            .collect()
    }
}

pub struct ParityDb {
    pub db: parity_db::Db,
    statistics_enabled: bool,
}

impl ParityDb {
    fn to_options(path: PathBuf, config: &ParityDbConfig) -> Options {
        Options {
            path,
            sync_wal: true,
            sync_data: true,
            stats: config.enable_statistics,
            salt: None,
            columns: DbColumn::create_column_options(CompressionType::Lz4),
            compression_threshold: [(0, 128)].into_iter().collect(),
        }
    }

    pub fn open(path: impl Into<PathBuf>, config: &ParityDbConfig) -> anyhow::Result<Self> {
        let opts = Self::to_options(path.into(), config);
        Ok(Self {
            db: Db::open_or_create(&opts)?,
            statistics_enabled: opts.stats,
        })
    }

    /// Returns an appropriate column variant based on the information
    /// in the Cid.
    fn choose_column(cid: &Cid) -> DbColumn {
        match cid.codec() {
            DAG_CBOR if cid.hash().code() == u64::from(Blake2b256) => {
                DbColumn::GraphDagCborBlake2b256
            }
            _ => DbColumn::GraphFull,
        }
    }

    fn read_from_column<K>(&self, key: K, column: DbColumn) -> anyhow::Result<Option<Vec<u8>>>
    where
        K: AsRef<[u8]>,
    {
        self.db
            .get(column as u8, key.as_ref())
            .map_err(|e| anyhow!("error from column {column}: {e}"))
    }

    fn write_to_column<K, V>(&self, key: K, value: V, column: DbColumn) -> anyhow::Result<()>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        let tx = [(column as u8, key.as_ref(), Some(value.as_ref().to_vec()))];
        self.db
            .commit(tx)
            .map_err(|e| anyhow!("error writing to column {column}: {e}"))
    }
}

impl SettingsStore for ParityDb {
    fn read_bin(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        self.read_from_column(key.as_bytes(), DbColumn::Settings)
    }

    fn write_bin(&self, key: &str, value: &[u8]) -> anyhow::Result<()> {
        self.write_to_column(key.as_bytes(), value, DbColumn::Settings)
    }

    fn exists(&self, key: &str) -> anyhow::Result<bool> {
        self.db
            .get_size(DbColumn::Settings as u8, key.as_bytes())
            .map(|size| size.is_some())
            .context("error checking if key exists")
    }

    fn setting_keys(&self) -> anyhow::Result<Vec<String>> {
        let mut iter = self.db.iter(DbColumn::Settings as u8)?;
        let mut keys = vec![];
        while let Some((key, _)) = iter.next()? {
            keys.push(String::from_utf8(key)?);
        }
        Ok(keys)
    }
}

impl Blockstore for ParityDb {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        let column = Self::choose_column(k);
        match column {
            DbColumn::GraphDagCborBlake2b256 | DbColumn::GraphFull => {
                self.read_from_column(k.to_bytes(), column)
            }
            DbColumn::Settings => panic!("invalid column for IPLD data"),
        }
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        let column = Self::choose_column(k);

        match column {
            // We can put the data directly into the database without any encoding.
            DbColumn::GraphDagCborBlake2b256 | DbColumn::GraphFull => {
                self.write_to_column(k.to_bytes(), block, column)
            }
            DbColumn::Settings => panic!("invalid column for IPLD data"),
        }
    }

    fn put_many_keyed<D, I>(&self, blocks: I) -> anyhow::Result<()>
    where
        Self: Sized,
        D: AsRef<[u8]>,
        I: IntoIterator<Item = (Cid, D)>,
    {
        let values = blocks.into_iter().map(|(k, v)| {
            let column = Self::choose_column(&k);
            (column, k.to_bytes(), v.as_ref().to_vec())
        });
        let tx = values
            .into_iter()
            .map(|(col, k, v)| (col as u8, Operation::Set(k, v)));
        self.db
            .commit_changes(tx)
            .map_err(|e| anyhow!("error bulk writing: {e}"))
    }
}

impl BitswapStoreRead for ParityDb {
    fn contains(&self, cid: &Cid) -> anyhow::Result<bool> {
        // We need to check both columns because we don't know which one
        // the data is in. The order is important because most data will
        // be in the [`DbColumn::GraphDagCborBlake2b256`] column and so
        // it directly affects performance. If this assumption ever changes
        // then this code should be modified accordingly.
        for column in [DbColumn::GraphDagCborBlake2b256, DbColumn::GraphFull] {
            if self
                .db
                .get_size(column as u8, &cid.to_bytes())
                .context("error checking if key exists")?
                .is_some()
            {
                return Ok(true);
            }
        }
        Ok(false)
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
    use cid::multihash::Code::Sha2_256;
    use cid::multihash::MultihashDigest;
    use fvm_ipld_encoding::IPLD_RAW;
    use nom::AsBytes;

    use crate::db::tests::db_utils::parity::TempParityDB;

    use super::*;

    #[test]
    fn write_read_different_columns_test() -> anyhow::Result<()> {
        let db = TempParityDB::new();
        let data = [
            b"h'nglui mglw'nafh".to_vec(),
            b"Cthulhu".to_vec(),
            b"R'lyeh wgah'nagl fhtagn!!".to_vec(),
        ];
        let cids = [
            Cid::new_v1(DAG_CBOR, Blake2b256.digest(&data[0])),
            Cid::new_v1(DAG_CBOR, Sha2_256.digest(&data[1])),
            Cid::new_v1(IPLD_RAW, Blake2b256.digest(&data[1])),
        ];

        let cases = [
            (DbColumn::GraphDagCborBlake2b256, cids[0], &data[0]),
            (DbColumn::GraphFull, cids[1], &data[1]),
            (DbColumn::GraphFull, cids[2], &data[2]),
        ];

        for (_, cid, data) in cases {
            db.put_keyed(&cid, data)?;
        }

        for (column, cid, data) in cases {
            let actual = db
                .read_from_column(cid.to_bytes(), column)?
                .expect("data not found");
            assert_eq!(data, actual.as_bytes());

            // assert that the data is NOT in the other column
            let other_column = match column {
                DbColumn::GraphDagCborBlake2b256 => DbColumn::GraphFull,
                DbColumn::GraphFull => DbColumn::GraphDagCborBlake2b256,
                DbColumn::Settings => panic!("invalid column for IPLD data"),
            };
            let actual = db.read_from_column(cid.to_bytes(), other_column)?;
            assert!(actual.is_none());

            // Blockstore API usage should be transparent
            let actual =
                fvm_ipld_blockstore::Blockstore::get(db.as_ref(), &cid)?.expect("data not found");
            assert_eq!(data, actual.as_slice());
        }

        // Check non-IPLD column as well
        db.write_to_column(b"dagon", b"bloop", DbColumn::Settings)?;
        let actual = db
            .read_from_column(b"dagon", DbColumn::Settings)?
            .expect("data not found");
        assert_eq!(b"bloop", actual.as_bytes());

        Ok(())
    }

    #[test]
    fn choose_column_test() {
        let data = [0u8; 32];
        let cases = [
            (
                Cid::new_v1(DAG_CBOR, Blake2b256.digest(&data)),
                DbColumn::GraphDagCborBlake2b256,
            ),
            (
                Cid::new_v1(fvm_ipld_encoding::CBOR, Blake2b256.digest(&data)),
                DbColumn::GraphFull,
            ),
            (
                Cid::new_v1(DAG_CBOR, cid::multihash::Code::Sha2_256.digest(&data)),
                DbColumn::GraphFull,
            ),
        ];

        for (cid, expected) in cases {
            let actual = ParityDb::choose_column(&cid);
            assert_eq!(expected, actual);
        }
    }
}
