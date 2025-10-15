// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    cli_shared::{chain_path, read_config},
    daemon::db_util::load_all_forest_cars,
    db::{
        CAR_DB_DIR_NAME,
        car::ManyCar,
        db_engine::{db_root, open_db},
    },
    networks::NetworkChain,
    shim::clock::ChainEpoch,
    tool::subcommands::api_cmd::generate_test_snapshot::ReadOpsTrackingStore,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// Compute state tree for an epoch
#[derive(Debug, clap::Args)]
pub struct StateComputeCommand {
    /// Which epoch to compute the state transition for
    #[arg(long, required = true)]
    epoch: ChainEpoch,
    /// Filecoin network chain
    #[arg(long, required = true)]
    chain: NetworkChain,
    /// Optional path to the database folder or a snapshot `CAR` file
    #[arg(long)]
    db: Option<PathBuf>,
    /// Optional path to the database snapshot `CAR` file to write to for reproducing the computation
    #[arg(long)]
    export_db: Option<PathBuf>,
}

impl StateComputeCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let Self {
            epoch,
            chain,
            db,
            export_db,
        } = self;
        let db_root = if let Some(db) = db {
            db
        } else {
            let (_, config) = read_config(None, Some(chain.clone()))?;
            db_root(&chain_path(&config))?
        };
        let (db, _tmp_dir) = if db_root.is_file() {
            let temp_parity_db_root = tempfile::tempdir()?;
            let db_writer = open_db(temp_parity_db_root.path().to_owned(), &Default::default())?;
            let db = ManyCar::new(db_writer);
            let forest_car_db_dir = db_root.join(CAR_DB_DIR_NAME);
            load_all_forest_cars(&db, &forest_car_db_dir)?;
            (
                Arc::new(ReadOpsTrackingStore::new(db)),
                Some(temp_parity_db_root),
            )
        } else {
            (
                super::api_cmd::generate_test_snapshot::load_db(&db_root)?,
                None,
            )
        };
        Ok(())
    }
}
