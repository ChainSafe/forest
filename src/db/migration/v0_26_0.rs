// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Migration logic for 0.25.1 to 0.26.0 version.
//! A new `Indices` column has been added to enable the storage of CID-indexed data.

use crate::blocks::TipsetKey;
use crate::db::db_engine::Db;
use crate::db::migration::migration_map::MigrationOperationExt as _;
use crate::db::migration::v0_26_0::paritydb_0_25_1::{DbColumn, ParityDb};
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

pub(super) struct Migration0_25_1_0_26_0 {
    from: Version,
    to: Version,
}

/// Migrates the database from version 0.25.1 to 0.26.0
impl MigrationOperation for Migration0_25_1_0_26_0 {
    fn new(from: Version, to: Version) -> Self
    where
        Self: Sized,
    {
        Self { from, to }
    }

    fn from(&self) -> &Version {
        &self.from
    }

    fn to(&self) -> &Version {
        &self.to
    }

    fn migrate_core(&self, chain_data_path: &Path, _: &Config) -> anyhow::Result<PathBuf> {
        let old_db = self.old_db_path(chain_data_path);
        let temp_db = self.temporary_db_path(chain_data_path);

        let old_car_db_path = old_db.join(CAR_DB_DIR_NAME);
        let temp_car_db_path = temp_db.join(CAR_DB_DIR_NAME);

        // Make sure `car_db` dir exists as it might not be the case when migrating
        // from older versions.
        if old_car_db_path.is_dir() {
            info!(
                "Copying snapshot from {} to {}",
                old_db.display(),
                temp_db.display()
            );

            fs_extra::copy_items(
                &[old_car_db_path.as_path()],
                temp_car_db_path,
                &CopyOptions::default().copy_inside(true),
            )?;
        }

        let db = ParityDb::open(old_db)?;

        // open the new database to migrate data from the old one.
        let new_db = paritydb_0_26_0::ParityDb::open(&temp_db)?;

        for col in DbColumn::iter() {
            info!("Migrating column {}", col);
            let mut res = anyhow::Ok(());
            match col {
                DbColumn::GraphDagCborBlake2b256 | DbColumn::PersistentGraph => {
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

        Ok(temp_db)
    }
}

/// Database settings from Forest `v0.25.1`
mod paritydb_0_25_1 {
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

/// Database settings from Forest `v0.26.0`
mod paritydb_0_26_0 {
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
        Indices,
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
                        DbColumn::Indices => parity_db::ColumnOptions {
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
