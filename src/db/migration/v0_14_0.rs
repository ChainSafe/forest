// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Migration logic for 0.15.1 to 0.16.0 version.
//! We are getting rid of rolling db in favor of mark-and-sweep GC. Therefore the two databases
//! previously representing node state have to be merged into a new one and removed.

use crate::db::db_engine::Db;
use crate::db::migration::migration_map::temporary_db_name;
use crate::db::migration::v0_14_0::paritydb_0_15_1::{DbColumn, ParityDb};
use anyhow::Context;
use cid::multihash::Code::Blake2b256;
use cid::multihash::MultihashDigest;
use cid::Cid;
use fvm_ipld_encoding::DAG_CBOR;
use semver::Version;
use std::path::{Path, PathBuf};
use strum::IntoEnumIterator;
use tracing::info;

use super::migration_map::MigrationOperation;

pub(super) struct Migration0_15_1_0_16_0 {
    from: Version,
    to: Version,
}

/// Migrates the database from version 0.15.1 to 0.16.0
/// This migration merges the two databases represented by rolling db into one.
impl MigrationOperation for Migration0_15_1_0_16_0 {
    fn new(from: Version, to: Version) -> Self
    where
        Self: Sized,
    {
        Self { from, to }
    }

    fn pre_checks(&self, _chain_data_path: &Path) -> anyhow::Result<()> {
        Ok(())
    }

    fn migrate(&self, chain_data_path: &Path) -> anyhow::Result<PathBuf> {
        let source_db = chain_data_path.join(self.from.to_string());

        let db_paths: Vec<PathBuf> = source_db
            .read_dir()?
            .filter_map(|entry| Some(entry.ok()?.path()))
            .filter(|entry| entry.is_dir())
            .collect();
        let temp_db_path = chain_data_path.join(temporary_db_name(&self.from, &self.to));
        if temp_db_path.exists() {
            info!(
                "removing old temporary database {temp_db_path}",
                temp_db_path = temp_db_path.display()
            );
            std::fs::remove_dir_all(&temp_db_path)?;
        }

        // open the new database to migrate data from the old one.
        let new_db = ParityDb::open(&temp_db_path)?;

        // because of the rolling db, we have to do the migration for each sub-database...
        for sub_db in &db_paths {
            info!("migrating RollingDB partition {:?}", sub_db);
            let db = ParityDb::open(sub_db)?;

            for col in DbColumn::iter() {
                info!("migrating column {}", col);
                let mut res = anyhow::Ok(());
                if col == DbColumn::GraphDagCborBlake2b256 {
                    db.db.iter_column_while(col as u8, |val| {
                        let hash = Blake2b256.digest(&val.value);
                        let cid = Cid::new_v1(DAG_CBOR, hash);
                        res = new_db
                            .db
                            .commit_changes([Db::set_operation(
                                col as u8,
                                cid.to_bytes(),
                                val.value,
                            )])
                            .context("failed to commit");

                        if res.is_err() {
                            return false;
                        }

                        true
                    })?;
                    res?;
                } else {
                    let mut iter = db.db.iter(col as u8)?;
                    while let Some((key, value)) = iter.next()? {
                        // We don't need this anymore as the old GC has been deprecated.
                        if key.eq(b"estimated_reachable_records") {
                            continue;
                        }
                        new_db
                            .db
                            .commit_changes([Db::set_operation(col as u8, key, value)])
                            .context("failed to commit")?;
                    }
                }
            }
        }

        drop(new_db);
        Ok(temp_db_path)
    }

    fn post_checks(&self, chain_data_path: &Path) -> anyhow::Result<()> {
        let temp_db_name = temporary_db_name(&self.from, &self.to);
        if !chain_data_path.join(&temp_db_name).exists() {
            anyhow::bail!(
                "migration database {} does not exist",
                chain_data_path.join(temp_db_name).display()
            );
        }
        Ok(())
    }
}

/// Database settings, Forest `v0.15.1`
mod paritydb_0_15_1 {
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
