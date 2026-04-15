// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::daemon::bundle::load_actor_bundles;
use crate::db::{
    car::ManyCar,
    db_engine::{Db, DbConfig},
};
use crate::networks::{ChainConfig, Height, NetworkChain};
use crate::state_migration::run_state_migrations;
use anyhow::Context as _;
use clap::Args;
use std::{path::PathBuf, sync::Arc};

/// Runs a single state migration against the head of a snapshot, using a
/// throwaway on-disk ParityDb as the writable backing store so that timings
/// reflect the real production I/O path.
///
/// The snapshot CAR file is attached as a read-only layer; any state tree
/// blocks produced by the migration are written to the temporary ParityDb,
/// which is removed when the command exits.
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
}

impl MigrateCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let Self { snapshot, height } = self;

        // On-disk ParityDb so the benchmark reflects production I/O rather than
        // the in-memory fast path.
        let temp_dir = tempfile::Builder::new()
            .prefix("forest-migrate-")
            .tempdir()?;
        let paritydb_path = temp_dir.path().join("paritydb");
        let paritydb = Db::open(&paritydb_path, &DbConfig::default())?;
        tracing::info!("Using temporary ParityDb at {}", paritydb_path.display());

        let store = Arc::new(ManyCar::new(paritydb));
        store
            .read_only_file(&snapshot)
            .with_context(|| format!("failed to attach snapshot {}", snapshot.display()))?;

        let head = store.heaviest_tipset()?;
        let genesis = head.genesis(&store)?;
        let network = NetworkChain::from_genesis(genesis.cid()).context(
            "snapshot genesis does not match any known mainnet/calibnet/butterflynet genesis; custom devnets are not supported",
        )?;
        let chain_config = ChainConfig::from_chain(&network);

        // The migration reads the target-height actor bundle from the
        // blockstore; load it into the writable layer so it's visible through
        // the ManyCar.
        load_actor_bundles(store.writer(), &network).await?;

        let epoch = chain_config.epoch(height);
        anyhow::ensure!(
            epoch > 0,
            "no epoch configured for height {height} on {network}"
        );

        let parent_state = *head.parent_state();
        tracing::info!(
            "Running {height} migration on {network} (epoch {epoch}); head epoch {head_epoch}, parent state {parent_state}",
            head_epoch = head.epoch(),
        );

        let start = std::time::Instant::now();
        let new_state = run_state_migrations(epoch, &chain_config, &store, &parent_state)?;
        let elapsed = start.elapsed();

        match new_state {
            Some(new_state) => {
                tracing::info!(
                    "Migration completed: {parent_state} -> {new_state} in {elapsed}",
                    elapsed = humantime::format_duration(elapsed),
                );
            }
            None => anyhow::bail!(
                "No migration ran. Check that the mapping for height {height} is registered for {network} in `get_migrations` and that the snapshot's head is compatible."
            ),
        }

        Ok(())
    }
}
