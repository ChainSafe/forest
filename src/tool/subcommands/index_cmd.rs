// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{path::PathBuf, sync::Arc};

use anyhow::bail;
use clap::Subcommand;

use crate::chain::ChainStore;
use crate::chain::index::ResolveNullTipset;
use crate::cli_shared::{chain_path, read_config};
use crate::daemon::db_util::load_all_forest_cars;
use crate::daemon::db_util::{RangeSpec, backfill_db};
use crate::db::CAR_DB_DIR_NAME;
use crate::db::car::ManyCar;
use crate::db::db_engine::{db_root, open_db};
use crate::genesis::read_genesis_header;
use crate::networks::NetworkChain;
use crate::shim::clock::ChainEpoch;
use crate::state_manager::StateManager;
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
        /// The starting tipset epoch for back-filling (inclusive), defaults to chain head
        #[arg(long)]
        from: Option<ChainEpoch>,
        /// Ending tipset epoch for back-filling (inclusive)
        #[arg(long)]
        to: Option<ChainEpoch>,
        /// Number of tipsets for back-filling
        #[arg(long, conflicts_with = "to")]
        n_tipsets: Option<usize>,
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
                n_tipsets,
            } => {
                let spec = match (to, n_tipsets) {
                    (Some(x), None) => RangeSpec::To(*x),
                    (None, Some(x)) => RangeSpec::NumTipsets(*x),
                    (None, None) => {
                        bail!("You must provide either '--to' or '--n-tipsets'.");
                    }
                    _ => unreachable!(), // Clap ensures this case is handled
                };

                let (_, config) = read_config(config.as_ref(), chain.clone())?;

                let chain_data_path = chain_path(&config);
                let db_root_dir = db_root(&chain_data_path)?;

                let db_writer = Arc::new(open_db(db_root_dir.clone(), config.db_config())?);
                let db = Arc::new(ManyCar::new(db_writer.clone()));
                let forest_car_db_dir = db_root_dir.join(CAR_DB_DIR_NAME);

                load_all_forest_cars(&db, &forest_car_db_dir)?;

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
                    db.clone(),
                    chain_config,
                    genesis_header.clone(),
                )?);

                let state_manager = Arc::new(StateManager::new(chain_store.clone())?);

                let head_ts = chain_store.heaviest_tipset();

                println!("Database path: {}", db_root_dir.display());
                println!("From epoch:    {}", from.unwrap_or_else(|| head_ts.epoch()));
                println!("{spec}");
                println!("Head epoch:    {}", head_ts.epoch());

                let from_ts = if let Some(from) = from {
                    // ensure from epoch is not greater than head epoch. This can happen if the
                    // assumed head is actually a null tipset.
                    let from = std::cmp::min(*from, head_ts.epoch());
                    chain_store.chain_index().tipset_by_height(
                        from,
                        head_ts,
                        ResolveNullTipset::TakeOlder,
                    )?
                } else {
                    head_ts
                };

                backfill_db(&state_manager, &from_ts, spec).await?;

                Ok(())
            }
        }
    }
}
