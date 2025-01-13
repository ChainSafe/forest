// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Migration logic for 0.22.0 to 0.22.1 version.
//! A `PersistentGraph` column has been introduced to allow for storage of persistent data that
//! isn't garbage collected. The initial use-case is network upgrade manifest storage.

use crate::blocks::TipsetKey;
use crate::db::db_engine::Db;
use crate::db::migration::migration_map::temporary_db_name;
use crate::db::migration::v0_22_1::paritydb_0_22_0::{DbColumn, ParityDb};
use crate::db::CAR_DB_DIR_NAME;
use crate::rpc::eth::types::EthHash;
use crate::utils::multihash::prelude::*;
use crate::Config;
use anyhow::Context;
use cid::Cid;
use fs_extra::dir::CopyOptions;
use fvm_ipld_encoding::DAG_CBOR;
use semver::Version;
use std::path::{Path, PathBuf};
use strum::IntoEnumIterator;
use tracing::info;

use super::migration_map::MigrationOperation;

pub(super) struct Migration0_22_0_0_22_1 {
    from: Version,
    to: Version,
}

/// Migrates the database from version 0.22.0 to 0.22.1
impl MigrationOperation for Migration0_22_0_0_22_1 {
    fn new(from: Version, to: Version) -> Self
    where
        Self: Sized,
    {
        Self { from, to }
    }

    fn pre_checks(&self, _chain_data_path: &Path) -> anyhow::Result<()> {
        Ok(())
    }

    fn migrate(&self, chain_data_path: &Path, _: &Config) -> anyhow::Result<PathBuf> {
        let source_db = chain_data_path.join(self.from.to_string());

        let temp_db_path = chain_data_path.join(temporary_db_name(&self.from, &self.to));
        if temp_db_path.exists() {
            info!(
                "Removing old temporary database {temp_db_path}",
                temp_db_path = temp_db_path.display()
            );
            std::fs::remove_dir_all(&temp_db_path)?;
        }

        let old_car_db_path = source_db.join(CAR_DB_DIR_NAME);
        let new_car_db_path = temp_db_path.join(CAR_DB_DIR_NAME);

        // Make sure `car_db` dir exists as it might not be the case when migrating
        // from older versions.
        if old_car_db_path.is_dir() {
            info!(
                "Copying snapshot from {source_db} to {temp_db_path}",
                source_db = old_car_db_path.display(),
                temp_db_path = new_car_db_path.display()
            );

            fs_extra::copy_items(
                &[old_car_db_path.as_path()],
                new_car_db_path,
                &CopyOptions::default().copy_inside(true),
            )?;
        }

        let db = ParityDb::open(source_db)?;

        // open the new database to migrate data from the old one.
        let new_db = paritydb_0_22_1::ParityDb::open(&temp_db_path)?;

        for col in DbColumn::iter() {
            info!("Migrating column {}", col);
            let mut res = anyhow::Ok(());
            match col {
                DbColumn::GraphDagCborBlake2b256 => {
                    db.db.iter_column_while(col as u8, |val| {
                        let hash = MultihashCode::Blake2b256.digest(&val.value);
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
                }
                DbColumn::EthMappings => {
                    db.db.iter_column_while(col as u8, |val| {
                        let tsk: Result<TipsetKey, fvm_ipld_encoding::Error> =
                            fvm_ipld_encoding::from_slice(&val.value);
                        if tsk.is_err() {
                            res = Err(tsk.context("serde error").unwrap_err());
                            return false;
                        }
                        let cid = tsk.unwrap().cid();

                        if cid.is_err() {
                            res = Err(cid.context("serde error").unwrap_err());
                            return false;
                        }

                        let hash: EthHash = cid.unwrap().into();
                        res = new_db
                            .db
                            .commit_changes([Db::set_operation(
                                col as u8,
                                hash.0.as_bytes().to_vec(),
                                val.value,
                            )])
                            .context("failed to commit");

                        if res.is_err() {
                            return false;
                        }

                        true
                    })?;
                    res?;
                }
                _ => {
                    let mut iter = db.db.iter(col as u8)?;
                    while let Some((key, value)) = iter.next()? {
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

/// Database settings from Forest `v0.22.0`
mod paritydb_0_22_0 {
    use parity_db::{CompressionType, Db, Options};
    use std::path::PathBuf;
    use strum::{Display, EnumIter, IntoEnumIterator};

    #[derive(Copy, Clone, Debug, PartialEq, EnumIter, Display)]
    #[repr(u8)]
    pub(super) enum DbColumn {
        GraphDagCborBlake2b256,
        GraphFull,
        Settings,
        EthMappings,
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

    pub(super) struct ParityDb {
        pub db: parity_db::Db,
    }

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

        pub(super) fn open(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
            let opts = Self::to_options(path.into());
            Ok(Self {
                db: Db::open_or_create(&opts)?,
            })
        }
    }
}

/// Database settings from Forest `v0.22.1`
mod paritydb_0_22_1 {
    use parity_db::{CompressionType, Db, Options};
    use std::path::PathBuf;
    use strum::{Display, EnumIter, IntoEnumIterator};

    #[derive(Copy, Clone, Debug, PartialEq, EnumIter, Display)]
    #[repr(u8)]
    pub(super) enum DbColumn {
        GraphDagCborBlake2b256,
        GraphFull,
        Settings,
        EthMappings,
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
                        DbColumn::Settings => {
                            parity_db::ColumnOptions {
                                // explicitly disable preimage for settings column
                                // othewise we are not able to overwrite entries
                                preimage: false,
                                // This is needed for key retrieval.
                                btree_index: true,
                                compression,
                                ..Default::default()
                            }
                        }
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

    pub(super) struct ParityDb {
        pub db: parity_db::Db,
    }

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

        pub(super) fn open(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
            let opts = Self::to_options(path.into());
            Ok(Self {
                db: Db::open_or_create(&opts)?,
            })
        }
    }
}
