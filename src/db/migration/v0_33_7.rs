// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Migration logic for databases with the v0.33.6 schema to v0.33.7.
//! An `EthBlockBloom` column has been added to store per-tipset Ethereum block logs blooms.

use super::migration_map::MigrationOperation;
use crate::Config;
use crate::db::migration::migration_map::MigrationOperationExt as _;
use anyhow::Context;
use semver::Version;
use std::path::{Path, PathBuf};
use tracing::info;

pub(super) struct Migration0_33_6_0_33_7 {
    from: Version,
    to: Version,
}

/// Migrates the database from version 0.33.6 to 0.33.7
impl MigrationOperation for Migration0_33_6_0_33_7 {
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

        info!("Adding EthBlockBloom column to database");
        let mut opts = paritydb_0_33_6::to_options(temp_db.clone());
        parity_db::Db::add_column(&mut opts, paritydb_0_33_6::eth_block_bloom_column_options())
            .context("failed to add EthBlockBloom column")?;

        info!("Migration completed successfully");
        Ok(temp_db)
    }
}

/// Database settings from Forest `v0.33.6`
mod paritydb_0_33_6 {
    use parity_db::{ColumnOptions, CompressionType, Options};
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
        fn create_column_options(compression: CompressionType) -> Vec<ColumnOptions> {
            DbColumn::iter()
                .map(|col| match col {
                    DbColumn::GraphDagCborBlake2b256 | DbColumn::PersistentGraph => ColumnOptions {
                        preimage: true,
                        compression,
                        ..Default::default()
                    },
                    DbColumn::GraphFull => ColumnOptions {
                        preimage: true,
                        btree_index: true,
                        compression,
                        ..Default::default()
                    },
                    DbColumn::Settings => ColumnOptions {
                        preimage: false,
                        btree_index: true,
                        compression,
                        ..Default::default()
                    },
                    DbColumn::EthMappings => ColumnOptions {
                        preimage: false,
                        btree_index: false,
                        compression,
                        ..Default::default()
                    },
                })
                .collect()
        }
    }

    /// Options for the `EthBlockBloom` column introduced in v0.33.7.
    pub(super) fn eth_block_bloom_column_options() -> ColumnOptions {
        ColumnOptions {
            preimage: false,
            btree_index: true,
            compression: CompressionType::Lz4,
            ..Default::default()
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
