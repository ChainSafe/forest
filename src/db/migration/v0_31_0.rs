// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Migration logic for databases with the v0.30.5 schema to v0.31.0.
//! The `Indices` column has been removed as events are now stored as `AMTs` in the blockstore.

use super::migration_map::MigrationOperation;
use crate::Config;
use crate::db::migration::migration_map::MigrationOperationExt as _;
use anyhow::Context;
use semver::Version;
use std::path::{Path, PathBuf};
use tracing::info;

pub(super) struct Migration0_30_5_0_31_0 {
    from: Version,
    to: Version,
}

/// Migrates the database from version 0.30.5 to 0.31.0
impl MigrationOperation for Migration0_30_5_0_31_0 {
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

        info!(
            "Renaming database directory from {} to {}",
            old_db.display(),
            temp_db.display()
        );
        std::fs::rename(&old_db, &temp_db).context("failed to rename database directory")?;

        // Create a placeholder so the delete step succeeds
        std::fs::create_dir_all(&old_db).context("failed to create placeholder directory")?;

        info!("Dropping last column (Indices) from database");
        let mut opts = paritydb_0_30_5::to_options(temp_db.clone());
        parity_db::Db::drop_last_column(&mut opts).context("failed to drop last column")?;

        info!("Migration completed successfully");
        Ok(temp_db)
    }
}

/// Database settings from Forest `v0.30.5`
mod paritydb_0_30_5 {
    use parity_db::{CompressionType, Options};
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
}
