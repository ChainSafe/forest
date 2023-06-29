// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ops::DerefMut;
use std::sync::atomic;
use std::{path::PathBuf, sync::Arc};

use super::errors::Error;
use crate::blocks::Tipset;
use crate::db::{parity_db_config::ParityDbConfig, DBStatistics, Dump, Store};
use crate::libp2p_bitswap::{BitswapStoreRead, BitswapStoreReadWrite};
use crate::utils::io::progress_bar::ProgressBarCurrentTotalPair;

use anyhow::anyhow;
use cid::multihash::Code::Blake2b256;
use cid::multihash::MultihashDigest;
use cid::Cid;
use futures_util::AsyncWriteExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_car::CarHeader;
use fvm_ipld_encoding::DAG_CBOR;
use lazy_static::lazy_static;
use once_cell::sync::Lazy;
use parity_db::{CompressionType, Db, Operation, Options};
use strum::{Display, EnumIter, FromRepr, IntoEnumIterator};
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

/// This is specific to Forest's `ParityDb` usage.
/// It is used to determine which column to use for a given entry type.
#[derive(Copy, Clone, Debug, Display, PartialEq, FromRepr, EnumIter)]
#[repr(u8)]
enum DbColumn {
    /// Column for storing IPLD data with `Blake2b256` hash and `DAG_CBOR` codec.
    /// Most entries in the `blockstore` will be stored in this column.
    GraphDagCborBlake2b256,
    /// Column for storing other IPLD data (different codec or hash function).
    /// It allows key retrieval at the cost of degraded performance. Given that
    /// there will be a small number of entries in this column, the performance
    /// degradation is negligible.
    GraphFull,
    /// Column for storing anything non-IPLD data. This column is not exportable.
    Other,
}

impl DbColumn {
    const fn is_exportable(&self) -> bool {
        match self {
            DbColumn::GraphDagCborBlake2b256 | DbColumn::GraphFull => true,
            DbColumn::Other => false,
        }
    }
}

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
        let compression = compression_type_from_str(&config.compression_type)?;
        Ok(Options {
            path,
            sync_wal: true,
            sync_data: true,
            stats: config.enable_statistics,
            salt: None,
            columns: vec![
                // GraphDagCborBlake2b256
                parity_db::ColumnOptions {
                    // The `preimage` flag tells ParityDB that a given value always has the same
                    // key. With this flag enabled, ParityDB can short-circuit insertions when the
                    // keys already exist in the database (as opposed to updating the value).
                    // Forest is exclusively storing IPLD data where the key is the hash of the
                    // value. While we try not to insert the same data multiple times, it does
                    // happen on occasion, and this flag improves the DB performance in those
                    // scenarios.
                    preimage: true,
                    compression,
                    ..Default::default()
                },
                // GraphFull
                parity_db::ColumnOptions {
                    preimage: true,
                    // This is needed for key retrieval.
                    btree_index: true,
                    compression,
                    ..Default::default()
                },
                // Other
                parity_db::ColumnOptions {
                    preimage: true,
                    compression,
                    ..Default::default()
                },
            ],
            compression_threshold: [(0, 128)].into_iter().collect(),
        })
    }

    pub fn open(path: impl Into<PathBuf>, config: &ParityDbConfig) -> anyhow::Result<Self> {
        let opts = Self::to_options(path.into(), config)?;
        Ok(Self {
            db: Arc::new(Db::open_or_create(&opts)?),
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

    fn read_column<K>(&self, key: K, column: DbColumn) -> anyhow::Result<Option<Vec<u8>>>
    where
        K: AsRef<[u8]>,
    {
        self.db
            .get(column as u8, key.as_ref())
            .map_err(|e| anyhow!("error from column {column}: {e}"))
    }

    fn write_column<K, V>(&self, key: K, value: V, column: DbColumn) -> anyhow::Result<()>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        let tx = [(column as u8, key.as_ref(), Some(value.as_ref().to_vec()))];
        self.db
            .commit(tx)
            .map_err(|e| anyhow!("error writing to column {column}: {e}"))
    }

    /// Poor man's check if the column is indexed. There doesn't seem to be
    /// an exposed API to do it cleanly.
    fn is_column_indexed(&self, column: DbColumn) -> bool {
        self.db.iter(column as u8).is_ok()
    }
}

/// The assumption is that the methods from this trait are not used in the
/// `Blockstore` context. All writes and reads through this interface will
/// go through a column dedicated for non-IPLD data.
impl Store for ParityDb {
    fn read<K>(&self, key: K) -> Result<Option<Vec<u8>>, Error>
    where
        K: AsRef<[u8]>,
    {
        self.read_column(key, DbColumn::Other).map_err(Error::from)
    }

    fn write<K, V>(&self, key: K, value: V) -> Result<(), Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.write_column(key, value, DbColumn::Other)
            .map_err(Error::from)
    }

    /// [`parity_db::Db::commit`] API is doing extra allocations on keys,
    /// See <https://docs.rs/crate::db/parity-db/0.4.3/source/src/db.rs>
    fn bulk_write(
        &self,
        values: impl IntoIterator<Item = (impl Into<Vec<u8>>, impl Into<Vec<u8>>)>,
    ) -> Result<(), Error> {
        let tx = values
            .into_iter()
            .map(|(k, v)| (DbColumn::Other as u8, Operation::Set(k.into(), v.into())));
        self.db.commit_changes(tx).map_err(Error::from)
        // <https://docs.rs/crate::db/parity-db/0.4.3/source/src/db.rs>
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

    fn exists<K>(&self, key: K) -> Result<bool, Error>
    where
        K: AsRef<[u8]>,
    {
        Ok(DbColumn::iter()
            .filter_map(|col| self.db.get_size(col as u8, key.as_ref()).ok())
            .any(|size| size.is_some()))
    }
}

impl Blockstore for ParityDb {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        let column = Self::choose_column(k);
        match column {
            DbColumn::GraphDagCborBlake2b256 | DbColumn::GraphFull => {
                self.read_column(k.to_bytes(), column)
            }
            DbColumn::Other => panic!("invalid column for IPLD data"),
        }
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        let column = Self::choose_column(k);

        match column {
            // We can put the data directly into the database without any encoding.
            DbColumn::GraphDagCborBlake2b256 | DbColumn::GraphFull => {
                self.write_column(k.to_bytes(), block, column)
            }
            DbColumn::Other => panic!("invalid column for IPLD data"),
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
            match column {
                DbColumn::GraphDagCborBlake2b256 | DbColumn::Other | DbColumn::GraphFull => {
                    (column, k.to_bytes(), v.as_ref().to_vec())
                }
            }
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

lazy_static! {
    pub static ref DATABASE_DUMP_PROGRESS: ProgressBarCurrentTotalPair = Default::default();
}

#[async_trait::async_trait]
impl Dump for ParityDb {
    fn total_exportable_entries(&self) -> anyhow::Result<u64> {
        // Get total number of exportable entries in the DB.
        // Theoretically, if the statistics are enabled we could get it
        // directly from the DB. In practice, the values turned out
        // to be slightly off. Plus, the statistics are normally disabled
        // for performance reasons.
        let mut total_entries = 0;

        for col in DbColumn::iter().filter(|col| col.is_exportable()) {
            if self.is_column_indexed(col) {
                let mut iter = self.db.iter(col as u8)?;
                while let Ok(Some(_)) = iter.next() {
                    total_entries += 1;
                }
            } else {
                self.db
                    .iter_column_while(col as u8, |_| {
                        total_entries += 1;
                        true
                    })
                    .map_err(|e| anyhow!("error iterating over column: {e}"))?;
            }
        }

        Ok(total_entries)
    }

    /// Exports selected columns from the database to a writer.
    /// Caveat: recent DB commits may not be exported due to internal limitations
    /// of the underlying database.
    async fn write_exportable<W>(&self, writer: W, tipset: &Tipset) -> anyhow::Result<()>
    where
        W: futures::AsyncWrite + Send + Unpin + 'static,
    {
        // We are modifying a global static variable, so we need to make sure
        // that only one export is in progress at any given time.
        // This should be handled on the RPC level, but we add this check
        // here as a safety measure.
        static LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
        let _locked = LOCK
            .try_lock()
            .map_err(|e| anyhow!("another export is in progress: {e}"))?;

        let total_entries = self.total_exportable_entries()?;
        info!("Exporting {} entries from the DB", total_entries);

        let progress_inc = || {
            DATABASE_DUMP_PROGRESS
                .0
                .fetch_add(1, atomic::Ordering::Relaxed);
        };

        DATABASE_DUMP_PROGRESS.0.store(0, atomic::Ordering::Relaxed);
        DATABASE_DUMP_PROGRESS
            .1
            .store(total_entries, atomic::Ordering::Relaxed);

        let writer = Arc::new(tokio::sync::Mutex::new(writer));
        let writer_clone = writer.clone();

        // This is a a bit arbitrary, increasing this does not significantly affect performance.
        const CHANNEL_CAP: usize = 1000;
        let (tx, rx) = flume::bounded(CHANNEL_CAP);
        let header = CarHeader::from(tipset.key().cids().to_vec());

        let mut tasks = JoinSet::new();

        tasks.spawn(async move {
            let mut writer = writer_clone.lock().await;
            let mut stream = rx.stream();
            let writer = writer.deref_mut();
            header
                .write_stream_async(writer, &mut stream)
                .await
                .map_err(|e| anyhow!("error writing to car file: {e}"))
        });

        // This may be a bit overkill with two columns, but the idea here is to have an exhaustive
        // match over all variants, so that we don't forget to handle a new column
        // in the export when we add it.
        for col in DbColumn::iter().filter(|col| col.is_exportable()) {
            let tx_clone = tx.clone();
            let db = self.db.clone();

            debug!("Starting to iterate the {col} column");
            match col {
                DbColumn::GraphDagCborBlake2b256 => {
                    tasks.spawn_blocking(move || {
                        let mut error_iterating = None;
                        db.iter_column_while(col as u8, |item| {
                            let cid = Cid::new_v1(DAG_CBOR, Blake2b256.digest(&item.value));
                            // an error means that all receivers have been dropped
                            // this means our write will fail, so we should stop iterating
                            if let Err(e) = tx_clone.send((cid, item.value)) {
                                error_iterating = Some(e);
                                false
                            } else {
                                progress_inc();
                                true
                            }
                        })?;

                        if let Some(e) = error_iterating {
                            Err(e.into())
                        } else {
                            Ok::<_, anyhow::Error>(())
                        }
                    });
                }
                DbColumn::GraphFull => {
                    tasks.spawn(async move {
                        let mut iter = db.iter(col as u8)?;
                        while let Ok(Some(entry)) = iter.next() {
                            let (cid, block) = (Cid::try_from(entry.0)?, entry.1);
                            tx_clone.send_async((cid, block)).await?;
                            progress_inc();
                        }
                        Ok::<_, anyhow::Error>(())
                    });
                }
                DbColumn::Other => {
                    panic!("Other column is not exportable");
                }
            }
            debug!("Finished iterating the {col} column");
        }

        drop(tx);

        while let Some(res) = tasks.join_next().await {
            let _out = res?;
        }

        let mut writer = writer.lock().await;
        writer.flush().await?;
        writer.close().await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use fvm_ipld_car::CarReader;
    use parity_db::CompressionType;
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    use crate::{
        blocks::BlockHeader, db::tests::db_utils::parity::TempParityDB, shim::address::Address,
    };

    use super::*;

    #[test]
    fn total_exportable_entries_with_stats_test() -> anyhow::Result<()> {
        let mut db = TempParityDB::new();
        let data = b"h'nglui mglw'nafh Cthulhu R'lyeh wgah'nagl fhtagn";

        assert_eq!(0, db.total_exportable_entries()?);

        // write to the simplified column, this should be counted
        db.put_keyed(&Cid::new_v1(DAG_CBOR, Blake2b256.digest(data)), data)?;

        db.force_flush();
        assert_eq!(1, db.total_exportable_entries()?);

        // write to the full column, this should be counted
        db.put_keyed(
            &Cid::new_v1(DAG_CBOR, cid::multihash::Code::Sha2_256.digest(data)),
            data,
        )?;
        db.force_flush();
        assert_eq!(2, db.total_exportable_entries()?);

        // write to the other column, this should NOT be counted
        db.write("dagon", "bloop")?;
        db.force_flush();
        assert_eq!(2, db.total_exportable_entries()?);

        Ok(())
    }

    // This test will:
    // * write important entries to the database,
    // * dump the database to a CAR file,
    // * read the CAR file and verify that the entries are present
    #[tokio::test]
    async fn write_exportable_test() -> anyhow::Result<()> {
        let mut db = TempParityDB::new();
        let data = [
            b"h'nglui mglw'nafh Cthulhu R'lyeh".to_vec(),
            b"wgah'nagl fhtagn".to_vec(),
        ];

        db.put_keyed(
            &Cid::new_v1(DAG_CBOR, Blake2b256.digest(&data[0])),
            &data[0],
        )?;
        db.put_keyed(
            &Cid::new_v1(DAG_CBOR, cid::multihash::Code::Sha2_256.digest(&data[1])),
            &data[1],
        )?;

        db.force_flush();

        let temp_path = tempfile::NamedTempFile::new()?;
        let car_file = tokio::fs::File::create(temp_path.path()).await?;

        let header = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .build()
            .unwrap();
        let tipset = Tipset::new(vec![header]).unwrap();

        db.write_exportable(car_file.compat_write(), &tipset)
            .await?;

        let file_reader = tokio::fs::File::open(temp_path.path()).await?;
        let mut car = CarReader::new(file_reader.compat()).await?;
        let header = &car.header;
        assert_eq!(header.roots.len(), 1);
        assert_eq!(header.roots[0], tipset.key().cids()[0]);

        let mut blocks = Vec::new();
        while let Some(block) = car.next_block().await? {
            blocks.push(block);
        }

        assert_eq!(blocks.len(), data.len());
        for (i, block) in blocks.iter().enumerate() {
            assert_eq!(block.data, data[i]);
        }

        Ok(())
    }

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
