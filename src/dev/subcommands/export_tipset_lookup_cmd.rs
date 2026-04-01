// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    chain::ChainStore,
    cli_shared::{chain_path, read_config},
    daemon::db_util::load_all_forest_cars,
    db::{
        CAR_DB_DIR_NAME, MemoryDB,
        car::ManyCar,
        db_engine::{db_root, open_db},
    },
    genesis::read_genesis_header,
    networks::{ChainConfig, NetworkChain},
    shim::clock::ChainEpoch,
};
use clap::Args;
use fil_actors_shared::fvm_ipld_amt::Amt;
use human_repr::HumanCount;
use std::{num::NonZeroUsize, path::PathBuf, sync::Arc, time::Instant};

/// Exports epoch to tipset key mapping AMT as a `ForestCAR` file for a given epoch range.
/// The exported AMT can be used to quickly look up the tipset key for a given epoch without traversing the chain,
/// which is useful for tools that need to access historical tipsets frequently.
#[derive(Debug, Args)]
pub struct ExportTipsetLookupCommand {
    /// Filecoin network chain (e.g., calibnet, mainnet)
    #[arg(long, required = true)]
    chain: NetworkChain,
    /// Optional path to the database folder
    #[arg(long)]
    db: Option<PathBuf>,
    /// Start epoch (inclusive). Defaults to the current chain head
    #[arg(long)]
    from: Option<ChainEpoch>,
    /// End epoch (inclusive).
    #[arg(long, default_value = "0")]
    to: ChainEpoch,
    /// Every N epochs to skip when exporting the AMT. Defaults to 1 (export every epoch)
    #[arg(long, default_value = "1")]
    skip_length: NonZeroUsize,
    /// The path to the output `ForestCAR` file
    #[arg(short, long)]
    output: PathBuf,
}

impl ExportTipsetLookupCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let Self {
            chain,
            db,
            from,
            to,
            skip_length,
            output,
        } = self;
        let skip_length = skip_length.get() as i64;
        let db_root_path = if let Some(db) = db {
            db
        } else {
            let (_, config) = read_config(None, Some(chain.clone()))?;
            db_root(&chain_path(&config))?
        };
        let forest_car_db_dir = db_root_path.join(CAR_DB_DIR_NAME);
        let db = Arc::new(ManyCar::new(open_db(db_root_path, &Default::default())?));
        load_all_forest_cars(&db, &forest_car_db_dir)?;

        let chain_config = Arc::new(ChainConfig::from_chain(&chain));
        let genesis_header =
            read_genesis_header(None, chain_config.genesis_bytes(&db).await?.as_deref(), &db)
                .await?;
        let chain_store = Arc::new(ChainStore::new(
            db.clone(),
            db.clone(),
            db.clone(),
            chain_config,
            genesis_header,
        )?);

        let head = chain_store.heaviest_tipset();

        let amt_db = Arc::new(MemoryDB::default());
        let mut amt = Amt::new(&amt_db);
        let start = Instant::now();
        for ts in head.chain(chain_store.blockstore()) {
            if let Some(from) = from
                && ts.epoch() > from
            {
                continue;
            }
            if ts.epoch() < to {
                break;
            }
            if ts.epoch() % skip_length != 0 {
                continue;
            }
            amt.set(ts.epoch() as u64, ts.key().clone())?;
        }
        let root = amt.flush()?;
        println!(
            "Exported tipset lookup AMT with root CID: {root}, len: {}, size: {}, took {}",
            amt_db.blockstore_len(),
            amt_db.blockstore_size_bytes().human_count_bytes(),
            humantime::format_duration(start.elapsed())
        );
        amt_db
            .export_forest_car_with_roots(
                nunny::vec![root],
                &mut tokio::fs::File::create(output).await?,
            )
            .await?;
        Ok(())
    }
}
