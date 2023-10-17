// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Migration logic from version 0.12.1
//! This is more of an exercise and a PoC than a real migration, the reason being that the
//! database directories before the Forest version 0.12.1 were not yet versioned and so we can only guess the version
//! of their underlying databases.
//! All in all, it gives us a good idea of how to do a migration and solves potential caveats
//! coming from the rolling database and ParityDb.

use crate::db::migration::migration_map::db_name;
use fs_extra::dir::CopyOptions;
use semver::Version;
use std::path::{Path, PathBuf};
use tracing::info;

use super::migration_map::MigrationOperation;

pub(super) struct Migration0_12_1_0_13_0 {
    from: Version,
    to: Version,
}

/// Migrates the database from version 0.12.1 to 0.13.0
/// This migration is needed because the `Settings` column in the `ParityDb` table changed to
/// binary-tree indexed and the data from `HEAD` and `meta.yaml` files was moved to the `Settings` column.
impl MigrationOperation for Migration0_12_1_0_13_0 {
    fn pre_checks(&self, _chain_data_path: &Path) -> anyhow::Result<()> {
        Ok(())
    }

    fn migrate(&self, chain_data_path: &Path) -> anyhow::Result<PathBuf> {
        let source_db = chain_data_path.join(self.from.to_string());

        let temp_db_path = chain_data_path.join(self.temporary_db_name());
        if temp_db_path.exists() {
            info!(
                "removing old temporary database {temp_db_path}",
                temp_db_path = temp_db_path.display()
            );
            std::fs::remove_dir_all(&temp_db_path)?;
        }

        // copy the old database to a new directory
        info!(
            "copying old database from {source_db} to {temp_db_path}",
            source_db = source_db.display(),
            temp_db_path = temp_db_path.display()
        );
        fs_extra::copy_items(
            &[source_db.as_path()],
            temp_db_path.clone(),
            &CopyOptions::default().copy_inside(true),
        )?;

        // there are two YAML files that are supposed to be in the `Settings` column now.
        // We need to add them manually to the database.
        let head_file_path = chain_data_path.join("HEAD");
        let head = std::fs::read(&head_file_path)?;

        // The HEAD type was kept in binary format, so we need to convert it to JSON.
        let head: Vec<cid::Cid> = fvm_ipld_encoding::from_slice(&head)?;
        let head = serde_json::to_vec(&head)?;

        // Estimated records were kept in a YAML file...
        let estimated_records_path = chain_data_path.join("meta.yaml");
        let estimated_records = std::fs::read_to_string(&estimated_records_path)?;
        let estimated_records = estimated_records
            .split_once(':')
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "meta.yaml file is not in the expected format, expected `key: value`"
                )
            })
            .map(|(_, value)| value.trim())?
            .parse::<u64>()?;
        let estimated_records = serde_json::to_vec(&estimated_records)?;

        // because of the rolling db, we have to do the migration for each sub-database...
        for sub_db in temp_db_path
            .read_dir()?
            .filter_map(|entry| Some(entry.ok()?.path()))
            .filter(|entry| entry.is_dir())
        {
            let db = paritydb_0_12_1::ParityDb::open(&sub_db)?;

            // The `Settings` column is now binary-tree indexed, there is only one entry in this version
            // that needs to be migrated.
            // It is very unlikely this entry was changed, but we treat it as the first migration proof of
            // concept.
            let mpool_config = db
                .db
                .get(paritydb_0_12_1::DbColumn::Settings as u8, b"/mpool/config")?;

            drop(db);

            let mut db_config = paritydb_0_12_1::ParityDb::to_options(sub_db.clone());
            let settings_column_config_new = parity_db::ColumnOptions {
                preimage: false,
                btree_index: true,
                compression: parity_db::CompressionType::Lz4,
                ..Default::default()
            };
            parity_db::Db::reset_column(
                &mut db_config,
                paritydb_0_12_1::DbColumn::Settings as u8,
                Some(settings_column_config_new),
            )?;

            let db = paritydb_0_13_0::ParityDb::open(&sub_db)?;

            let tx = [(
                paritydb_0_13_0::DbColumn::Settings as u8,
                b"/mpool/config",
                mpool_config,
            )];
            db.db.commit(tx)?;

            let tx = [(
                paritydb_0_13_0::DbColumn::Settings as u8,
                b"head",
                Some(head.clone()),
            )];
            db.db.commit(tx)?;

            let tx = [(
                paritydb_0_13_0::DbColumn::Settings as u8,
                b"estimated_reachable_records",
                Some(estimated_records.clone()),
            )];
            db.db.commit(tx)?;
        }

        std::fs::remove_file(head_file_path)?;
        std::fs::remove_file(estimated_records_path)?;

        Ok(temp_db_path)
    }

    fn post_checks(&self, chain_data_path: &Path) -> anyhow::Result<()> {
        let temp_db_name = self.temporary_db_name();
        if !chain_data_path.join(&temp_db_name).exists() {
            anyhow::bail!(
                "migration database {} does not exist",
                chain_data_path.join(temp_db_name).display()
            );
        }
        Ok(())
    }

    fn new(from: Version, to: Version) -> Self
    where
        Self: Sized,
    {
        Self { from, to }
    }

    fn temporary_db_name(&self) -> String {
        db_name(&self.from, &self.to)
    }
}

/// Database settings from Forest `v0.12.1`
mod paritydb_0_12_1 {
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
                        DbColumn::GraphDagCborBlake2b256 | DbColumn::Settings => {
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

/// Database settings from Forest `v0.13.0`
mod paritydb_0_13_0 {

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
