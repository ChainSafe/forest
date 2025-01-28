// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Migration logic for 0.18.0 to 0.19.0 version.
//! For the need for Ethereum RPC API, a new column in parity-db has been introduced to handle
//! mapping of values such as, `Hash` to `TipsetKey` and `Hash` to message `Cid`.

use crate::chain::ChainStore;
use crate::cli_shared::chain_path;
use crate::daemon::db_util::{load_all_forest_cars, populate_eth_mappings};
use crate::db::car::ManyCar;
use crate::db::db_engine::Db;
use crate::db::migration::migration_map::temporary_db_name;
use crate::db::migration::v0_19_0::paritydb_0_18_0::{DbColumn, ParityDb};
use crate::db::CAR_DB_DIR_NAME;
use crate::genesis::read_genesis_header;
use crate::networks::ChainConfig;
use crate::state_manager::StateManager;
use crate::utils::multihash::prelude::*;
use crate::{db, Config};
use anyhow::Context as _;
use cid::Cid;
use fs_extra::dir::CopyOptions;
use fvm_ipld_encoding::DAG_CBOR;
use semver::Version;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use strum::IntoEnumIterator;
use tracing::info;

use super::migration_map::MigrationOperation;

pub(super) struct Migration0_18_0_0_19_0 {
    from: Version,
    to: Version,
}

/// Migrates the database from version 0.18.0 to 0.19.0
impl MigrationOperation for Migration0_18_0_0_19_0 {
    fn new(from: Version, to: Version) -> Self
    where
        Self: Sized,
    {
        Self { from, to }
    }

    fn pre_checks(&self, _chain_data_path: &Path) -> anyhow::Result<()> {
        Ok(())
    }

    fn migrate(&self, chain_data_path: &Path, config: &Config) -> anyhow::Result<PathBuf> {
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
        let new_db = paritydb_0_19_0::ParityDb::open(&temp_db_path)?;

        for col in DbColumn::iter() {
            info!("Migrating column {}", col);
            let mut res = anyhow::Ok(());
            if col == DbColumn::GraphDagCborBlake2b256 {
                db.db.iter_column_while(col as u8, |val| {
                    let hash = MultihashCode::Blake2b256.digest(&val.value);
                    let cid = Cid::new_v1(DAG_CBOR, hash);
                    res = new_db
                        .db
                        .commit_changes([Db::set_operation(col as u8, cid.to_bytes(), val.value)])
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
                    new_db
                        .db
                        .commit_changes([Db::set_operation(col as u8, key, value)])
                        .context("failed to commit")?;
                }
            }
        }

        drop(new_db);

        // open the new database to populate the Ethereum mappings
        let handle = tokio::runtime::Handle::current();
        futures::executor::block_on(async {
            let mut cloned_config = config.clone();
            let mut data_dir: PathBuf = chain_data_path.into();
            data_dir.pop();
            cloned_config.client.data_dir = data_dir;
            let db_name = temporary_db_name(&self.from, &self.to);
            handle
                .spawn(create_state_manager_and_populate(cloned_config, db_name))
                .await
                .expect("Task spawned panicked")
        })?;

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

async fn create_state_manager_and_populate(config: Config, db_name: String) -> anyhow::Result<()> {
    use db::parity_db::ParityDb as ParityDbCurrent;

    let chain_data_path = chain_path(&config);
    let db_root_dir = chain_data_path.join(db_name);
    let db = ParityDbCurrent::wrap(
        paritydb_0_19_0::ParityDb::open(db_root_dir.clone())?.db,
        false,
        true,
    );
    let db_writer = Arc::new(db);
    let db = Arc::new(ManyCar::new(db_writer.clone()));
    let forest_car_db_dir = db_root_dir.join(CAR_DB_DIR_NAME);
    load_all_forest_cars(&db, &forest_car_db_dir)?;

    let chain_config = Arc::new(ChainConfig::from_chain(config.chain()));

    let genesis_header = read_genesis_header(
        config.client.genesis_file.as_deref(),
        chain_config.genesis_bytes(&db).await?.as_deref(),
        &db,
    )
    .await?;

    let chain_store = Arc::new(ChainStore::new(
        Arc::clone(&db),
        db.writer().clone(),
        db.writer().clone(),
        chain_config.clone(),
        genesis_header.clone(),
    )?);

    let state_manager = StateManager::new(
        Arc::clone(&chain_store),
        chain_config,
        Arc::new(config.sync.clone()),
    )?;

    populate_eth_mappings(&state_manager, &chain_store.heaviest_tipset())?;

    Ok(())
}

/// Database settings from Forest `v0.18.0`
mod paritydb_0_18_0 {
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

/// Database settings from Forest `v0.19.0`
mod paritydb_0_19_0 {
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
