// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Migration logic for 0.13.0 -> 0.13.1 versions.
//! We are getting rid of RollingDB in favour of mark-and-sweep GC. Therefore the two databases
//! previously representing node state have to be merged into a new one and removed.
//! TODO: Make sure we don't want the progress bar for GC anymore and drop estimated number of records.

use crate::db::db_engine::Db;
use crate::db::migration::v0_13_0::paritydb_0_13_0::{DbColumn, ParityDb};
use cid::multihash::Code::Blake2b256;
use cid::multihash::MultihashDigest;
use cid::Cid;
use fvm_ipld_encoding::DAG_CBOR;
use std::path::{Path, PathBuf};
use strum::IntoEnumIterator;
use tracing::info;

use super::migration_map::MigrationOperation;

#[derive(Default)]
pub(super) struct Migration0_13_0_0_13_1;

/// Temporary database path for the migration.
const MIGRATION_DB_0_13_0_0_13_1: &str = "migration_0_13_0_to_0_13_1";

/// Transaction batch size.
const TX_BATCH_SIZE: usize = 10_000;

/// Migrates the database from version 0.13.0 to 0.13.1
/// This migration merges the two databases represented by RollingDB into one.
impl MigrationOperation for Migration0_13_0_0_13_1 {
    fn pre_checks(&self, _chain_data_path: &Path) -> anyhow::Result<()> {
        Ok(())
    }

    fn migrate(&self, chain_data_path: &Path) -> anyhow::Result<PathBuf> {
        let source_db = chain_data_path.join("0.13.0");

        let db_paths: Vec<PathBuf> = source_db
            .read_dir()?
            .filter_map(|entry| Some(entry.ok()?.path()))
            .filter(|entry| entry.is_dir())
            .collect();
        let temp_db_path = chain_data_path.join(MIGRATION_DB_0_13_0_0_13_1);
        if temp_db_path.exists() {
            info!(
                "removing old temporary database {temp_db_path}",
                temp_db_path = temp_db_path.display()
            );
            std::fs::remove_dir_all(&temp_db_path)?;
        }

        // open the new database to migrate data from the old one.
        let new_db = ParityDb::open(temp_db_path.join("0.13.1"))?;

        // because of the rolling db, we have to do the migration for each sub-database...
        for sub_db in &db_paths {
            info!("migrating RollingDB partition {:?}", sub_db);
            let mut vec = Vec::with_capacity(TX_BATCH_SIZE);
            let db = ParityDb::open(&sub_db)?;

            for col in DbColumn::iter() {
                info!("migrating column {}", col);
                let mut res = anyhow::Ok(());
                let mut records = 0;
                if col == DbColumn::GraphDagCborBlake2b256 {
                    db.db.iter_column_while(col as u8, |val| {
                        let hash = Blake2b256.digest(&val.value);
                        let cid = Cid::new_v1(DAG_CBOR, hash);

                        vec.push(Db::set_operation(col as u8, cid.to_bytes(), val.value));
                        records += 1;
                        if vec.len() == TX_BATCH_SIZE {
                            res = new_db.commit_changes(&mut vec);
                            if res.is_err() {
                                return false;
                            }
                            info!("migrated {} records in {} column", records, col);
                        }
                        true
                    })?;
                    res?;
                } else {
                    let mut iter = db.db.iter(col as u8)?;
                    while let Some((key, value)) = iter.next()? {
                        vec.push(Db::set_operation(col as u8, key, value));
                        records += 1;
                        if vec.len() == TX_BATCH_SIZE {
                            new_db.commit_changes(&mut vec)?;
                            info!("migrated {} records in {} column", records, col);
                        }
                    }
                }
                // Commit the leftovers.
                if vec.len() > 0 {
                    new_db.commit_changes(&mut vec)?;
                    info!("migrated {} records in {} column", records, col);
                }
            }
        }

        Ok(temp_db_path)
    }

    fn post_checks(&self, chain_data_path: &Path) -> anyhow::Result<()> {
        if !chain_data_path.join(MIGRATION_DB_0_13_0_0_13_1).exists() {
            anyhow::bail!(
                "migration database {} does not exist",
                chain_data_path.join(MIGRATION_DB_0_13_0_0_13_1).display()
            );
        }
        Ok(())
    }
}

/// Database settings, Forest `v0.13.0`
mod paritydb_0_13_0 {
    use crate::db;
    use parity_db::{CompressionType, Db, Options};
    use std::path::PathBuf;
    use strum::{Display, EnumIter, IntoEnumIterator};

    #[derive(Copy, Clone, Debug, PartialEq, EnumIter, Display)]
    #[repr(u8)]
    pub(super) enum DbColumn {
        GraphDagCborBlake2b256,
        GraphFull,
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

    pub(super) struct ParityDb {}

    impl ParityDb {
        pub(super) fn to_options(path: PathBuf) -> Options {
            Options {
                path,
                sync_wal: true,
                sync_data: true,
                stats: false,
                salt: None,
                columns: DbColumn::create_column_options(CompressionType::Lz4),
                compression_threshold: [(0, 128)].into_iter().collect(),
            }
        }

        // Return latest ParityDB implementation here to avoid too much repetition. This will break
        // if it changes and then this migration should either be maintained or removed.
        pub(super) fn open(path: impl Into<PathBuf>) -> anyhow::Result<db::parity_db::ParityDb> {
            let opts = Self::to_options(path.into());
            let db = db::parity_db::ParityDb::wrap(Db::open_or_create(&opts)?, false);
            Ok(db)
        }
    }
}
