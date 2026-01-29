// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    blocks::Tipset,
    chain::{ChainStore, index::ResolveNullTipset},
    chain_sync::{load_full_tipset, tipset_syncer::validate_tipset},
    cli_shared::{chain_path, read_config},
    db::{SettingsStoreExt, db_engine::db_root},
    genesis::read_genesis_header,
    interpreter::VMTrace,
    networks::{ChainConfig, NetworkChain},
    shim::clock::ChainEpoch,
    state_manager::{StateManager, StateOutput},
    tool::subcommands::api_cmd::generate_test_snapshot,
};
use nonzero_ext::nonzero;
use std::{num::NonZeroUsize, path::PathBuf, sync::Arc, time::Instant};

/// Interact with Filecoin chain state
#[derive(Debug, clap::Subcommand)]
pub enum StateCommand {
    Compute(ComputeCommand),
    ReplayCompute(ReplayComputeCommand),
    Validate(ValidateCommand),
    ReplayValidate(ReplayValidateCommand),
}

impl StateCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Compute(cmd) => cmd.run().await,
            Self::ReplayCompute(cmd) => cmd.run().await,
            Self::Validate(cmd) => cmd.run().await,
            Self::ReplayValidate(cmd) => cmd.run().await,
        }
    }
}

/// Compute state tree for an epoch
#[derive(Debug, clap::Args)]
pub struct ComputeCommand {
    /// Which epoch to compute the state transition for
    #[arg(long, required = true)]
    epoch: ChainEpoch,
    /// Filecoin network chain
    #[arg(long, required = true)]
    chain: NetworkChain,
    /// Optional path to the database folder
    #[arg(long)]
    db: Option<PathBuf>,
    /// Optional path to the database snapshot `CAR` file to write to for reproducing the computation
    #[arg(long)]
    export_db_to: Option<PathBuf>,
}

impl ComputeCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let Self {
            epoch,
            chain,
            db,
            export_db_to,
        } = self;
        disable_tipset_cache();
        let db_root_path = if let Some(db) = db {
            db
        } else {
            let (_, config) = read_config(None, Some(chain.clone()))?;
            db_root(&chain_path(&config))?
        };
        let db = generate_test_snapshot::load_db(&db_root_path, Some(&chain)).await?;
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
        let (ts, ts_next) = {
            // We don't want to track all entries that are visited by `tipset_by_height`
            db.pause_tracking();
            let ts = chain_store.chain_index().tipset_by_height(
                epoch,
                chain_store.heaviest_tipset(),
                ResolveNullTipset::TakeOlder,
            )?;
            let ts_next = chain_store.chain_index().tipset_by_height(
                epoch + 1,
                chain_store.heaviest_tipset(),
                ResolveNullTipset::TakeNewer,
            )?;
            db.resume_tracking();
            SettingsStoreExt::write_obj(
                &db.tracker,
                crate::db::setting_keys::HEAD_KEY,
                ts_next.key(),
            )?;
            // Only track the desired tipsets
            (
                Tipset::load_required(&db, ts.key())?,
                Tipset::load_required(&db, ts_next.key())?,
            )
        };
        let epoch = ts.epoch();
        let state_manager = Arc::new(StateManager::new(chain_store)?);

        let StateOutput {
            state_root,
            receipt_root,
            ..
        } = state_manager
            .compute_tipset_state(ts, crate::state_manager::NO_CALLBACK, VMTrace::NotTraced)
            .await?;
        let mut db_snapshot = vec![];
        db.export_forest_car(&mut db_snapshot).await?;
        println!(
            "epoch: {epoch}, state_root: {state_root}, receipt_root: {receipt_root}, db_snapshot_size: {}",
            human_bytes::human_bytes(db_snapshot.len() as f64)
        );
        let expected_state_root = *ts_next.parent_state();
        let expected_receipt_root = *ts_next.parent_message_receipts();
        anyhow::ensure!(
            state_root == expected_state_root,
            "state root mismatch, state_root: {state_root}, expected_state_root: {expected_state_root}"
        );
        anyhow::ensure!(
            receipt_root == expected_receipt_root,
            "receipt root mismatch, receipt_root: {receipt_root}, expected_receipt_root: {expected_receipt_root}"
        );
        if let Some(export_db_to) = export_db_to {
            std::fs::write(export_db_to, db_snapshot)?;
        }
        Ok(())
    }
}

/// Replay state computation with a db snapshot
/// To be used in conjunction with `forest-dev state compute`.
#[derive(Debug, clap::Args)]
pub struct ReplayComputeCommand {
    /// Path to the database snapshot `CAR` file generated by `forest-dev state compute`
    snapshot: PathBuf,
    /// Filecoin network chain
    #[arg(long, required = true)]
    chain: NetworkChain,
    /// Number of times to repeat the state computation
    #[arg(short, long, default_value_t = nonzero!(1usize))]
    n: NonZeroUsize,
}

impl ReplayComputeCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let Self { snapshot, chain, n } = self;
        let (sm, ts, ts_next) =
            crate::state_manager::utils::state_compute::prepare_state_compute(&chain, &snapshot)
                .await?;
        for _ in 0..n.get() {
            crate::state_manager::utils::state_compute::state_compute(&sm, ts.clone(), &ts_next)
                .await?;
        }
        Ok(())
    }
}

/// Validate tipset at a certain epoch
#[derive(Debug, clap::Args)]
pub struct ValidateCommand {
    /// Tipset epoch to validate
    #[arg(long, required = true)]
    epoch: ChainEpoch,
    /// Filecoin network chain
    #[arg(long, required = true)]
    chain: NetworkChain,
    /// Optional path to the database folder
    #[arg(long)]
    db: Option<PathBuf>,
    /// Optional path to the database snapshot `CAR` file to write to for reproducing the computation
    #[arg(long)]
    export_db_to: Option<PathBuf>,
}

impl ValidateCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let Self {
            epoch,
            chain,
            db,
            export_db_to,
        } = self;
        disable_tipset_cache();
        let db_root_path = if let Some(db) = db {
            db
        } else {
            let (_, config) = read_config(None, Some(chain.clone()))?;
            db_root(&chain_path(&config))?
        };
        let db = generate_test_snapshot::load_db(&db_root_path, Some(&chain)).await?;
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
        let ts = {
            // We don't want to track all entries that are visited by `tipset_by_height`
            db.pause_tracking();
            let ts = chain_store.chain_index().tipset_by_height(
                epoch,
                chain_store.heaviest_tipset(),
                ResolveNullTipset::TakeOlder,
            )?;
            db.resume_tracking();
            SettingsStoreExt::write_obj(&db.tracker, crate::db::setting_keys::HEAD_KEY, ts.key())?;
            // Only track the desired tipset
            Tipset::load_required(&db, ts.key())?
        };
        let epoch = ts.epoch();
        let fts = load_full_tipset(&chain_store, ts.key())?;
        let state_manager = Arc::new(StateManager::new(chain_store)?);
        validate_tipset(&state_manager, fts, None).await?;
        let mut db_snapshot = vec![];
        db.export_forest_car(&mut db_snapshot).await?;
        println!(
            "epoch: {epoch}, db_snapshot_size: {}",
            human_bytes::human_bytes(db_snapshot.len() as f64)
        );
        if let Some(export_db_to) = export_db_to {
            std::fs::write(export_db_to, db_snapshot)?;
        }
        Ok(())
    }
}

/// Replay tipset validation with a db snapshot
/// To be used in conjunction with `forest-dev state validate`.
#[derive(Debug, clap::Args)]
pub struct ReplayValidateCommand {
    /// Path to the database snapshot `CAR` file generated by `forest-dev state validate`
    snapshot: PathBuf,
    /// Filecoin network chain
    #[arg(long, required = true)]
    chain: NetworkChain,
    /// Number of times to repeat the state computation
    #[arg(short, long, default_value_t = nonzero!(1usize))]
    n: NonZeroUsize,
}

impl ReplayValidateCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let Self { snapshot, chain, n } = self;
        let (sm, fts) =
            crate::state_manager::utils::state_compute::prepare_state_validate(&chain, &snapshot)
                .await?;
        let epoch = fts.epoch();
        for _ in 0..n.get() {
            let fts = fts.clone();
            let start = Instant::now();
            validate_tipset(&sm, fts, None).await?;
            println!(
                "epoch: {epoch}, took {}.",
                humantime::format_duration(start.elapsed())
            );
        }
        Ok(())
    }
}

fn disable_tipset_cache() {
    unsafe {
        std::env::set_var("FOREST_TIPSET_CACHE_DISABLED", "1");
    }
}
