// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{EthMappingsStore, PersistentStore, SettingsStore};
use crate::cid_collections::CidHashSet;
use crate::db::{DBStatistics, GarbageCollectable, parity_db_config::ParityDbConfig};
use crate::libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};
use crate::rpc::eth::types::EthHash;
use crate::utils::multihash::prelude::*;
use anyhow::{Context as _, anyhow};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::DAG_CBOR;
use parity_db::{CompressionType, Db, Operation, Options};
use std::path::PathBuf;
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
    /// Column for storing Ethereum mappings.
    EthMappings,
    /// Column for storing IPLD data that has to be ignored by the garbage collector.
    /// Anything stored in this column can be considered permanent, unless manually
    /// deleted.
    PersistentGraph,
}

impl DbColumn {
    fn create_column_options(compression: CompressionType) -> Vec<parity_db::ColumnOptions> {
        DbColumn::iter()
            .map(|col| {
                match col {
                    DbColumn::GraphDagCborBlake2b256 | DbColumn::PersistentGraph => {
                        parity_db::ColumnOptions {
                            preimage: true,
                            compression,
                            ..Default::default()
                        }
                    }
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
                    DbColumn::EthMappings => parity_db::ColumnOptions {
                        preimage: false,
                        btree_index: false,
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
    // This is needed to maintain backwards-compatibility for pre-persistent-column migrations.
    disable_persistent_fallback: bool,
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
            disable_persistent_fallback: false,
        })
    }

    /// Returns an appropriate column variant based on the information
    /// in the Cid.
    fn choose_column(cid: &Cid) -> DbColumn {
        match cid.codec() {
            DAG_CBOR if cid.hash().code() == u64::from(MultihashCode::Blake2b256) => {
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

impl EthMappingsStore for ParityDb {
    fn read_bin(&self, key: &EthHash) -> anyhow::Result<Option<Vec<u8>>> {
        self.read_from_column(key.0.as_bytes(), DbColumn::EthMappings)
    }

    fn write_bin(&self, key: &EthHash, value: &[u8]) -> anyhow::Result<()> {
        self.write_to_column(key.0.as_bytes(), value, DbColumn::EthMappings)
    }

    fn exists(&self, key: &EthHash) -> anyhow::Result<bool> {
        self.db
            .get_size(DbColumn::EthMappings as u8, key.0.as_bytes())
            .map(|size| size.is_some())
            .context("error checking if key exists")
    }

    fn get_message_cids(&self) -> anyhow::Result<Vec<(Cid, u64)>> {
        let mut cids = Vec::new();

        self.db
            .iter_column_while(DbColumn::EthMappings as u8, |val| {
                if let Ok(value) = fvm_ipld_encoding::from_slice::<(Cid, u64)>(&val.value) {
                    cids.push(value);
                }
                true
            })?;

        Ok(cids)
    }

    fn delete(&self, keys: Vec<EthHash>) -> anyhow::Result<()> {
        Ok(self.db.commit_changes(keys.into_iter().map(|key| {
            let bytes = key.0.as_bytes().to_vec();
            (DbColumn::EthMappings as u8, Operation::Dereference(bytes))
        }))?)
    }
}

impl Blockstore for ParityDb {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        let column = Self::choose_column(k);
        let res = self.read_from_column(k.to_bytes(), column)?;
        if res.is_some() {
            return Ok(res);
        }
        self.get_persistent(k)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        let column = Self::choose_column(k);

        // We can put the data directly into the database without any encoding.
        self.write_to_column(k.to_bytes(), block, column)
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

impl PersistentStore for ParityDb {
    fn put_keyed_persistent(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        self.write_to_column(k.to_bytes(), block, DbColumn::PersistentGraph)
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
    type Hashes = MultihashCode;

    fn insert(&self, block: &crate::libp2p_bitswap::Block64<Self::Hashes>) -> anyhow::Result<()> {
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

type Op = (u8, Operation<Vec<u8>, Vec<u8>>);

impl ParityDb {
    /// Removes a record.
    ///
    /// # Arguments
    /// * `key` - record identifier
    pub fn dereference_operation(key: &Cid) -> Op {
        let column = Self::choose_column(key);
        (column as u8, Operation::Dereference(key.to_bytes()))
    }

    /// Updates/inserts a record.
    ///
    /// # Arguments
    /// * `column` - column identifier
    /// * `key` - record identifier
    /// * `value` - record contents
    pub fn set_operation(column: u8, key: Vec<u8>, value: Vec<u8>) -> Op {
        (column, Operation::Set(key, value))
    }

    // Get data from persistent graph column.
    fn get_persistent(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        if self.disable_persistent_fallback {
            return Ok(None);
        }
        self.read_from_column(k.to_bytes(), DbColumn::PersistentGraph)
    }
}

impl GarbageCollectable<CidHashSet> for ParityDb {
    fn get_keys(&self) -> anyhow::Result<CidHashSet> {
        let mut set = CidHashSet::new();

        // First iterate over all the indexed entries.
        let mut iter = self.db.iter(DbColumn::GraphFull as u8)?;
        while let Some((key, _)) = iter.next()? {
            let cid = Cid::try_from(key)?;
            set.insert(cid);
        }

        self.db
            .iter_column_while(DbColumn::GraphDagCborBlake2b256 as u8, |val| {
                let hash = MultihashCode::Blake2b256.digest(&val.value);
                let cid = Cid::new_v1(DAG_CBOR, hash);
                set.insert(cid);
                true
            })?;

        Ok(set)
    }

    fn remove_keys(&self, keys: CidHashSet) -> anyhow::Result<u32> {
        let mut iter = self.db.iter(DbColumn::GraphFull as u8)?;
        // It's easier to store cid's scheduled for removal directly as an `Op` to avoid costly
        // conversion with allocation.
        let mut deref_vec = Vec::new();
        while let Some((key, _)) = iter.next()? {
            let cid = Cid::try_from(key)?;

            if keys.contains(&cid) {
                deref_vec.push(Self::dereference_operation(&cid));
            }
        }

        self.db
            .iter_column_while(DbColumn::GraphDagCborBlake2b256 as u8, |val| {
                let hash = MultihashCode::Blake2b256.digest(&val.value);
                let cid = Cid::new_v1(DAG_CBOR, hash);

                if keys.contains(&cid) {
                    deref_vec.push(Self::dereference_operation(&cid));
                }
                true
            })?;

        let deleted: u32 = deref_vec.len().try_into()?;

        self.db.commit_changes(deref_vec).context("error remove")?;

        Ok(deleted)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::db::tests::db_utils::parity::TempParityDB;
    use fvm_ipld_encoding::IPLD_RAW;
    use nom::AsBytes;
    use std::ops::Deref;

    #[test]
    fn write_read_different_columns_test() {
        let db = TempParityDB::new();
        let data = [
            b"h'nglui mglw'nafh".to_vec(),
            b"Cthulhu".to_vec(),
            b"R'lyeh wgah'nagl fhtagn!!".to_vec(),
        ];
        let cids = [
            Cid::new_v1(DAG_CBOR, MultihashCode::Blake2b256.digest(&data[0])),
            Cid::new_v1(DAG_CBOR, MultihashCode::Sha2_256.digest(&data[1])),
            Cid::new_v1(IPLD_RAW, MultihashCode::Blake2b256.digest(&data[1])),
        ];

        let cases = [
            (DbColumn::GraphDagCborBlake2b256, cids[0], &data[0]),
            (DbColumn::GraphFull, cids[1], &data[1]),
            (DbColumn::GraphFull, cids[2], &data[2]),
        ];

        for (_, cid, data) in cases {
            db.put_keyed(&cid, data).unwrap();
        }

        for (column, cid, data) in cases {
            let actual = db
                .read_from_column(cid.to_bytes(), column)
                .unwrap()
                .expect("data not found");
            assert_eq!(data, actual.as_bytes());

            // assert that the data is NOT in the other column
            let other_column = match column {
                DbColumn::GraphDagCborBlake2b256 => DbColumn::GraphFull,
                DbColumn::GraphFull => DbColumn::GraphDagCborBlake2b256,
                DbColumn::Settings => panic!("invalid column for IPLD data"),
                DbColumn::EthMappings => panic!("invalid column for IPLD data"),
                DbColumn::PersistentGraph => panic!("invalid column for GC enabled IPLD data"),
            };
            let actual = db.read_from_column(cid.to_bytes(), other_column).unwrap();
            assert!(actual.is_none());

            // Blockstore API usage should be transparent
            let actual = fvm_ipld_blockstore::Blockstore::get(db.as_ref(), &cid)
                .unwrap()
                .expect("data not found");
            assert_eq!(data, actual.as_slice());
        }

        // Check non-IPLD column as well
        db.write_to_column(b"dagon", b"bloop", DbColumn::Settings)
            .unwrap();
        let actual = db
            .read_from_column(b"dagon", DbColumn::Settings)
            .unwrap()
            .expect("data not found");
        assert_eq!(b"bloop", actual.as_bytes());
    }

    #[test]
    #[ignore]
    // This needs to be reinstated once there is a reliable way to make sure that all the commits
    // make it to the database and are visible when read through iterator.
    // There seems to be a bug related to database reads.
    // See https://github.com/paritytech/parity-db/issues/227.
    fn garbage_collectable() {
        let db = TempParityDB::new();
        let data = [
            b"h'nglui mglw'nafh".to_vec(),
            b"Cthulhu".to_vec(),
            b"R'lyeh wgah'nagl fhtagn!!".to_vec(),
        ];
        let cids = [
            Cid::new_v1(DAG_CBOR, MultihashCode::Blake2b256.digest(&data[0])),
            Cid::new_v1(DAG_CBOR, MultihashCode::Sha2_256.digest(&data[1])),
            Cid::new_v1(IPLD_RAW, MultihashCode::Blake2b256.digest(&data[1])),
        ];

        let cases = [
            (DbColumn::GraphDagCborBlake2b256, cids[0], &data[0]),
            (DbColumn::GraphFull, cids[1], &data[1]),
            (DbColumn::GraphFull, cids[2], &data[2]),
        ];

        for (_, cid, data) in cases {
            db.put_keyed(&cid, data).unwrap();
        }

        let keys = db.get_keys().unwrap();

        // This is flaky, because iterating columns does not give visibility guarantees for the
        // latest commits.
        assert_eq!(keys.len(), cases.len());

        db.remove_keys(keys).unwrap();

        // Panics on this line: https://github.com/paritytech/parity-db/blob/ec686930169b84d21336bed6d6f05c787a17d61f/src/file.rs#L130
        let keys = db.get_keys().unwrap();
        assert_eq!(keys.len(), 0);
    }

    #[test]
    fn choose_column_test() {
        let data = [0u8; 32];
        let cases = [
            (
                Cid::new_v1(DAG_CBOR, MultihashCode::Blake2b256.digest(&data)),
                DbColumn::GraphDagCborBlake2b256,
            ),
            (
                Cid::new_v1(
                    fvm_ipld_encoding::CBOR,
                    MultihashCode::Blake2b256.digest(&data),
                ),
                DbColumn::GraphFull,
            ),
            (
                Cid::new_v1(DAG_CBOR, MultihashCode::Sha2_256.digest(&data)),
                DbColumn::GraphFull,
            ),
        ];

        for (cid, expected) in cases {
            let actual = ParityDb::choose_column(&cid);
            assert_eq!(expected, actual);
        }
    }

    #[test]
    fn persistent_tests() {
        let db = TempParityDB::new();
        let data = [
            b"h'nglui mglw'nafh".to_vec(),
            b"Cthulhu".to_vec(),
            b"R'lyeh wgah'nagl fhtagn!!".to_vec(),
        ];

        let persistent_data = data
            .clone()
            .into_iter()
            .map(|mut entry| {
                entry.push(255);
                entry
            })
            .collect::<Vec<Vec<u8>>>();

        let cids = [
            Cid::new_v1(DAG_CBOR, MultihashCode::Blake2b256.digest(&data[0])),
            Cid::new_v1(DAG_CBOR, MultihashCode::Sha2_256.digest(&data[1])),
            Cid::new_v1(IPLD_RAW, MultihashCode::Blake2b256.digest(&data[1])),
        ];

        for idx in 0..3 {
            let cid = &cids[idx];
            let persistent_entry = &persistent_data[idx];
            let data_entry = &data[idx];
            db.put_keyed_persistent(cid, persistent_entry).unwrap();
            // Check that we get persistent data if the data is otherwise absent from the GC enabled
            // storage.
            assert_eq!(
                Blockstore::get(db.deref(), cid).unwrap(),
                Some(persistent_entry.clone())
            );
            assert!(
                db.read_from_column(cid.to_bytes(), DbColumn::PersistentGraph)
                    .unwrap()
                    .is_some()
            );
            db.put_keyed(cid, data_entry).unwrap();
            assert_eq!(
                Blockstore::get(db.deref(), cid).unwrap(),
                Some(data_entry.clone())
            );
        }
    }
}
