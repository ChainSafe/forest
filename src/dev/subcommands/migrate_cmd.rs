// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Tipset;
use crate::daemon::bundle::load_actor_bundles;
use crate::db::{
    car::{AnyCar, ManyCar},
    db_engine::{Db, DbConfig},
};
use crate::networks::{ChainConfig, Height, NetworkChain};
use crate::state_migration::run_state_migrations;
use crate::utils::db::car_util::load_car;
use anyhow::Context as _;
use clap::{Args, ValueEnum};
use fvm_ipld_blockstore::Blockstore;
use std::{path::PathBuf, sync::Arc};

/// Read-side layout for the snapshot during the benchmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Backend {
    /// Attach the snapshot CAR as a read-only overlay on top of the temporary
    /// ParityDb. Migration reads hit the CAR layer.
    Car,
    /// Ingest the snapshot into the temporary ParityDb before timing the
    /// migration, so migration reads go through the writable db the same way
    /// a long-running daemon would.
    Db,
}

/// Runs a single state migration against the head of a snapshot, using a
/// throwaway on-disk ParityDb as the writable backing store so that timings
/// reflect the real production I/O path. The temporary ParityDb is removed
/// when the command exits.
#[derive(Debug, Args)]
pub struct MigrateCommand {
    /// Path to the snapshot CAR file (plain `.car` or zstd-compressed `.car.zst`).
    #[arg(long, required = true)]
    snapshot: PathBuf,
    /// Migration height to run (e.g. `GoldenWeek`, `Xxx`). The migration will
    /// be invoked as if the chain had reached that height's configured epoch
    /// for the network detected from the snapshot's genesis.
    #[arg(long, required = true)]
    height: Height,
    /// Storage layout to benchmark against.
    #[arg(long, value_enum, default_value_t = Backend::Car)]
    backend: Backend,
}

impl MigrateCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let Self {
            snapshot,
            height,
            backend,
        } = self;

        // On-disk ParityDb so the benchmark reflects production I/O rather than
        // the in-memory fast path.
        let temp_dir = tempfile::Builder::new()
            .prefix("forest-migrate-")
            .tempdir()?;
        let paritydb_path = temp_dir.path().join("paritydb");
        let paritydb = Db::open(&paritydb_path, &DbConfig::default())?;
        tracing::info!("Using temporary ParityDb at {}", paritydb_path.display());

        match backend {
            Backend::Db => {
                // The snapshot is about to be consumed into the writable db, so
                // identify the network first and hold on to the head tipset.
                let (head, network) = {
                    let car = AnyCar::try_from(snapshot.as_path()).with_context(|| {
                        format!("failed to open snapshot {}", snapshot.display())
                    })?;
                    let head = car.heaviest_tipset()?;
                    let network = detect_network(&head, &car)?;
                    (head, network)
                };
                let chain_config = ChainConfig::from_chain(&network);
                ensure_epoch(&chain_config, height, &network)?;

                tracing::info!("Importing snapshot into temporary ParityDb…");
                let import_start = std::time::Instant::now();
                let file = tokio::fs::File::open(&snapshot)
                    .await
                    .with_context(|| format!("failed to open {}", snapshot.display()))?;
                load_car(&paritydb, tokio::io::BufReader::new(file)).await?;
                tracing::info!(
                    "Snapshot imported in {}",
                    humantime::format_duration(import_start.elapsed())
                );
                let store = Arc::new(paritydb);
                load_actor_bundles(&*store, &network).await?;
                bench(&store, &chain_config, &network, head, height)
            }
            Backend::Car => {
                let store = Arc::new(ManyCar::new(paritydb));
                store
                    .read_only_file(&snapshot)
                    .with_context(|| format!("failed to attach snapshot {}", snapshot.display()))?;
                let head = store.heaviest_tipset()?;
                let network = detect_network(&head, &store)?;
                let chain_config = ChainConfig::from_chain(&network);
                ensure_epoch(&chain_config, height, &network)?;
                load_actor_bundles(store.writer(), &network).await?;
                bench(&store, &chain_config, &network, head, height)
            }
        }
    }
}

fn detect_network(head: &Tipset, store: &impl Blockstore) -> anyhow::Result<NetworkChain> {
    let genesis = head.genesis(store)?;
    NetworkChain::from_genesis(genesis.cid()).context(
        "snapshot genesis does not match any known mainnet/calibnet/butterflynet genesis; custom devnets are not supported",
    )
}

fn ensure_epoch(
    chain_config: &ChainConfig,
    height: Height,
    network: &NetworkChain,
) -> anyhow::Result<()> {
    let epoch = chain_config.epoch(height);
    anyhow::ensure!(
        epoch > 0,
        "no epoch configured for height {height} on {network}"
    );
    Ok(())
}

fn bench<DB: Blockstore + Send + Sync>(
    store: &Arc<DB>,
    chain_config: &ChainConfig,
    network: &NetworkChain,
    head: Tipset,
    height: Height,
) -> anyhow::Result<()> {
    let epoch = chain_config.epoch(height);
    let parent_state = *head.parent_state();
    tracing::info!(
        "Running {height} migration on {network} (epoch {epoch}); head epoch {head_epoch}, parent state {parent_state}",
        head_epoch = head.epoch(),
    );

    let start = std::time::Instant::now();
    let new_state = run_state_migrations(epoch, chain_config, store, &parent_state)?;
    let elapsed = start.elapsed();

    match new_state {
        Some(new_state) => tracing::info!(
            "Migration completed: {parent_state} -> {new_state} in {elapsed}",
            elapsed = humantime::format_duration(elapsed),
        ),
        None => anyhow::bail!(
            "No migration ran. Check that the mapping for height {height} is registered for {network} in `get_migrations` and that the snapshot's head is compatible."
        ),
    }
    Ok(())
}
