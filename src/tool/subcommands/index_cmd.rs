// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{path::PathBuf, sync::Arc};

use clap::Subcommand;

use crate::chain::ChainStore;
use crate::cli_shared::{chain_path, read_config};
use crate::daemon::db_util::load_all_forest_cars;
use crate::db::car::ManyCar;
use crate::db::db_engine::{db_root, open_db};
use crate::db::CAR_DB_DIR_NAME;
use crate::genesis::read_genesis_header;
use crate::interpreter::VMEvent;
use crate::interpreter::VMTrace;
use crate::networks::NetworkChain;
use crate::shim::clock::ChainEpoch;
use crate::state_manager::StateManager;
use crate::state_manager::NO_CALLBACK;
use crate::tool::offline_server::server::handle_chain_config;

#[derive(Debug, Subcommand)]
pub enum IndexCommands {
    /// Backfill index with Ethereum mappings, events, etc.
    Backfill {
        /// Optional TOML file containing forest daemon configuration
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Optional chain, will override the chain section of configuration file if used
        #[arg(long)]
        chain: Option<NetworkChain>,
        /// The starting tipset epoch for backfilling (inclusive)
        #[arg(long)]
        from: ChainEpoch,
        /// The ending tipset epoch for backfilling (inclusive)
        #[arg(long)]
        to: ChainEpoch,
    },
}

impl IndexCommands {
    pub async fn run(&self) -> anyhow::Result<()> {
        match self {
            Self::Backfill {
                config,
                chain,
                from,
                to,
            } => {
                let (_, config) = read_config(config.as_ref(), chain.clone())?;

                let chain_data_path = chain_path(&config);
                let db_root_dir = db_root(&chain_data_path)?;
                println!("Database path: {}", db_root_dir.display());
                println!("From epoch:    {}", from);
                println!("To epoch:      {}", to);

                let db_writer = Arc::new(open_db(db_root_dir.clone(), config.db_config().clone())?);
                let db = Arc::new(ManyCar::new(db_writer.clone()));
                let forest_car_db_dir = db_root_dir.join(CAR_DB_DIR_NAME);

                load_all_forest_cars(&db, &forest_car_db_dir)?;
                let head_ts = db.heaviest_tipset()?;

                let chain_config = Arc::new(handle_chain_config(&config.chain)?);
                let genesis_header = read_genesis_header(
                    None,
                    chain_config.genesis_bytes(&db).await?.as_deref(),
                    &db,
                )
                .await?;

                let chain_store = Arc::new(ChainStore::new(
                    db.clone(),
                    db.clone(),
                    db.writer().clone(),
                    db.writer().clone(),
                    chain_config.clone(),
                    genesis_header.clone(),
                )?);

                let state_manager = Arc::new(StateManager::new(chain_store.clone(), chain_config)?);

                println!("Head epoch:    {}", head_ts.epoch());

                for ts in head_ts
                    .clone()
                    .chain(&state_manager.chain_store().blockstore())
                {
                    let epoch = ts.epoch();
                    if epoch < *to {
                        break;
                    }
                    let tsk = ts.key().clone();

                    let state_output = state_manager
                        .compute_tipset_state(
                            Arc::new(ts),
                            NO_CALLBACK,
                            VMTrace::NotTraced,
                            VMEvent::PushedEventsRoot,
                        )
                        .await?;
                    for events_root in state_output.events_roots.iter() {
                        println!("Indexing events root @{}: {}", epoch, events_root);

                        chain_store.put_index(events_root, &tsk)?;
                    }
                }

                Ok(())
            }
        }
    }
}
