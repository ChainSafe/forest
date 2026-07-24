// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Tipset;
use crate::db::SettingsStoreExt;
use crate::db::car::forest::{
    FOREST_CAR_FILE_EXTENSION, TEMP_FOREST_CAR_FILE_EXTENSION, new_forest_car_temp_path_in,
};
use crate::db::car::{ForestCar, ManyCar};
use crate::ipld::ChainExportState;
use crate::message::SignedMessage;
use crate::networks::ChainConfig;
use crate::prelude::*;
use crate::rpc::sync::SnapshotProgressTracker;
use crate::shim::clock::ChainEpoch;
use crate::shim::policy::policy_constants::CHAIN_FINALITY;
use crate::state_manager::StateManager;
use crate::utils::db::car_stream::CarStream;
use crate::utils::io::EitherMmapOrRandomAccessFile;
use crate::utils::net::{DownloadFileOption, download_to};
use anyhow::{Context, bail};
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicI64, Ordering};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    time,
};
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast::error::TryRecvError;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use url::Url;
use walkdir::WalkDir;

#[cfg(doc)]
use crate::rpc::eth::types::EthHash;

#[cfg(doc)]
use crate::blocks::TipsetKey;

#[cfg(doc)]
use cid::Cid;

/// Loads all `.forest.car.zst` snapshots and cleanup stale `.forest.car.zst.tmp` files.
pub fn load_all_forest_cars_with_cleanup<T>(
    store: &ManyCar<T>,
    forest_car_db_dir: &Path,
) -> anyhow::Result<()> {
    load_all_forest_cars_internal(store, forest_car_db_dir, true)
}

/// Loads all `.forest.car.zst` snapshots
pub fn load_all_forest_cars<T>(store: &ManyCar<T>, forest_car_db_dir: &Path) -> anyhow::Result<()> {
    load_all_forest_cars_internal(store, forest_car_db_dir, false)
}

fn load_all_forest_cars_internal<T>(
    store: &ManyCar<T>,
    forest_car_db_dir: &Path,
    cleanup: bool,
) -> anyhow::Result<()> {
    if !forest_car_db_dir.is_dir() {
        fs::create_dir_all(forest_car_db_dir)?;
    }
    for file in WalkDir::new(forest_car_db_dir)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| {
            e.ok().and_then(|e| {
                if !e.file_type().is_dir() {
                    Some(e.into_path())
                } else {
                    None
                }
            })
        })
    {
        if let Some(filename) = file.file_name().and_then(OsStr::to_str) {
            if filename.ends_with(FOREST_CAR_FILE_EXTENSION) {
                let car = ForestCar::try_from(file.as_path())
                    .with_context(|| format!("Error loading car DB at {}", file.display()))?;
                store.read_only(car.into())?;
                debug!("Loaded car DB at {}", file.display());
            } else if cleanup && filename.ends_with(TEMP_FOREST_CAR_FILE_EXTENSION) {
                // Only delete files that appear to be incomplete car DB files
                match std::fs::remove_file(&file) {
                    Ok(_) => {
                        info!("Deleted temp car DB at {}", file.display());
                    }
                    Err(e) => {
                        warn!("Failed to delete temp car DB at {}: {e}", file.display());
                    }
                }
            }
        }
    }

    tracing::info!("Loaded {} CARs", store.len());

    Ok(())
}

#[derive(
    Default,
    PartialEq,
    Eq,
    Debug,
    Clone,
    Copy,
    strum::Display,
    strum::EnumString,
    Serialize,
    Deserialize,
)]
#[strum(serialize_all = "lowercase")]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub enum ImportMode {
    #[default]
    /// Hard link the snapshot and fallback to `Copy` if not applicable
    Auto,
    /// Copies the snapshot to the database directory.
    Copy,
    /// Moves the snapshot to the database directory (or copies and deletes the original).
    Move,
    /// Creates a symbolic link to the snapshot in the database directory.
    Symlink,
    /// Creates a symbolic link to the snapshot in the database directory.
    Hardlink,
}

/// This function validates and stores the CAR binary from `from_path`(either local path or URL) into the `{DB_ROOT}/car_db/`
/// (automatically trans-code into `.forest.car.zst` format when needed), and returns its final file path and the heaviest tipset.
pub async fn import_chain_as_forest_car(
    from_path: &Path,
    forest_car_db_dir: &Path,
    import_mode: ImportMode,
    rpc_endpoint: Url,
    f3_root: &Path,
    chain_config: &ChainConfig,
    snapshot_progress_tracker: &SnapshotProgressTracker,
) -> anyhow::Result<(PathBuf, Tipset)> {
    info!("Importing chain from snapshot at: {}", from_path.display());

    let stopwatch = time::Instant::now();

    let forest_car_db_path = forest_car_db_dir.join(format!(
        "{}{FOREST_CAR_FILE_EXTENSION}",
        chrono::Utc::now().timestamp_millis()
    ));

    let move_or_copy = |mode: ImportMode| {
        let forest_car_db_path = forest_car_db_path.clone();
        async move {
            let downloaded_car_temp_path = new_forest_car_temp_path_in(forest_car_db_dir)?;
            if let Ok(url) = Url::parse(&from_path.display().to_string()) {
                download_to(
                    &url,
                    &downloaded_car_temp_path,
                    DownloadFileOption::Resumable,
                    snapshot_progress_tracker.create_callback(),
                )
                .await?;

                snapshot_progress_tracker.completed();
            } else {
                snapshot_progress_tracker.not_required();
                if ForestCar::is_valid(&EitherMmapOrRandomAccessFile::open(from_path)?) {
                    move_or_copy_file(from_path, &downloaded_car_temp_path, mode)?;
                } else {
                    // For a local snapshot, we transcode directly instead of copying & transcoding.
                    transcode_into_forest_car(from_path, &downloaded_car_temp_path).await?;
                    if mode == ImportMode::Move {
                        std::fs::remove_file(from_path).context("Error removing original file")?;
                    }
                }
            }

            if ForestCar::is_valid(&EitherMmapOrRandomAccessFile::open(
                &downloaded_car_temp_path,
            )?) {
                downloaded_car_temp_path.persist(&forest_car_db_path)?;
            } else {
                // Use another temp file to make sure all final `.forest.car.zst` files are complete and valid.
                let forest_car_db_temp_path = new_forest_car_temp_path_in(forest_car_db_dir)?;
                transcode_into_forest_car(&downloaded_car_temp_path, &forest_car_db_temp_path)
                    .await?;
                forest_car_db_temp_path.persist(&forest_car_db_path)?;
            }
            anyhow::Ok(())
        }
    };

    match import_mode {
        ImportMode::Auto => {
            if Url::parse(&from_path.display().to_string()).is_ok() {
                // Fallback to move if from_path is url
                move_or_copy(ImportMode::Move).await?;
            } else if ForestCar::is_valid(&EitherMmapOrRandomAccessFile::open(from_path)?) {
                tracing::info!(
                    "Hardlinking {} to {}",
                    from_path.display(),
                    forest_car_db_path.display()
                );
                if std::fs::hard_link(from_path, &forest_car_db_path).is_err() {
                    tracing::warn!("Error creating hardlink, fallback to copy");
                    move_or_copy(ImportMode::Copy).await?;
                }
            } else {
                tracing::warn!(
                    "Snapshot file is not a valid forest.car.zst file, fallback to copy"
                );
                move_or_copy(ImportMode::Copy).await?;
            }
        }
        ImportMode::Copy | ImportMode::Move => {
            move_or_copy(import_mode).await?;
        }
        ImportMode::Symlink => {
            let from_path = std::path::absolute(from_path)?;
            if ForestCar::is_valid(&EitherMmapOrRandomAccessFile::open(&from_path)?) {
                tracing::info!(
                    "Symlinking {} to {}",
                    from_path.display(),
                    forest_car_db_path.display()
                );
                std::os::unix::fs::symlink(from_path, &forest_car_db_path)
                    .context("Error creating symlink")?;
            } else {
                bail!("Snapshot file must be a valid forest.car.zst file");
            }
        }
        ImportMode::Hardlink => {
            if ForestCar::is_valid(&EitherMmapOrRandomAccessFile::open(from_path)?) {
                tracing::info!(
                    "Hardlinking {} to {}",
                    from_path.display(),
                    forest_car_db_path.display()
                );
                std::fs::hard_link(from_path, &forest_car_db_path)
                    .context("Error creating hardlink")?;
            } else {
                bail!("Snapshot file must be a valid forest.car.zst file");
            }
        }
    };

    let forest_car = ForestCar::try_from(forest_car_db_path.as_path())?;

    if let Some(f3_cid) = forest_car.metadata().and_then(|m| m.f3_data) {
        if crate::f3::get_f3_sidecar_params(chain_config)
            .initial_power_table
            .is_none()
        {
            // To avoid importing old/wrong F3 data without initial power table check
            tracing::warn!(
                "skipped importing F3 data as the initial power table CID is not set in the current manifest"
            );
        } else {
            let mut f3_data = forest_car
                .get_reader(f3_cid)?
                .with_context(|| format!("f3 data not found, cid: {f3_cid}"))?;
            let mut temp_f3_snap = tempfile::Builder::new()
                .suffix(".f3snap.bin")
                .tempfile_in(forest_car_db_dir)?;
            {
                let f = temp_f3_snap.as_file_mut();
                std::io::copy(&mut f3_data, f)?;
                f.sync_all()?;
            }
            if let Err(e) = crate::f3::import_f3_snapshot(
                chain_config,
                rpc_endpoint.to_string(),
                f3_root.display().to_string(),
                temp_f3_snap.path().display().to_string(),
            ) {
                // Do not make it a hard error if anything is wrong with F3 snapshot
                tracing::error!("Failed to import F3 snapshot: {e:#}");
            }
        }
    }

    let ts = forest_car.heaviest_tipset()?;
    info!(
        "Imported snapshot in: {}s, heaviest tipset epoch: {}, key: {}",
        stopwatch.elapsed().as_secs(),
        ts.epoch(),
        ts.key()
    );

    Ok((forest_car_db_path, ts))
}

fn move_or_copy_file(from: &Path, to: &Path, import_mode: ImportMode) -> anyhow::Result<()> {
    match import_mode {
        ImportMode::Move => {
            tracing::info!("Moving {} to {}", from.display(), to.display());
            if fs::rename(from, to).is_ok() {
                Ok(())
            } else {
                fs::copy(from, to).context("Error copying file")?;
                fs::remove_file(from).context("Error removing original file")?;
                Ok(())
            }
        }
        ImportMode::Copy => {
            tracing::info!("Copying {} to {}", from.display(), to.display());
            fs::copy(from, to).map(|_| ()).context("Error copying file")
        }
        m => {
            bail!("{m} must be handled elsewhere");
        }
    }
}

async fn transcode_into_forest_car(from: &Path, to: &Path) -> anyhow::Result<()> {
    tracing::info!(
        from = %from.display(),
        to = %to.display(),
        "transcoding into forest car"
    );
    let car_stream = CarStream::new_from_path(from).await?;
    let roots = car_stream.header_v1.roots.clone();

    let mut writer = tokio::io::BufWriter::new(tokio::fs::File::create(to).await?);
    let frames = crate::db::car::forest::Encoder::compress_stream_default(
        car_stream.map_err(anyhow::Error::from),
    );
    crate::db::car::forest::Encoder::write(&mut writer, roots, frames).await?;
    writer.shutdown().await?;

    Ok(())
}

/// Settings-store key under which index backfill persists the epoch of the last committed
/// batch, so an interrupted backfill can be resumed from where it left off.
pub const BACKFILL_CHECKPOINT_KEY: &str = "/index/backfill/checkpoint";

/// Outcome of indexing a single tipset during backfill.
enum ProcessOutcome {
    /// The tipset was indexed.
    Indexed,
    /// The tipset was skipped because its state output was unavailable and recomputation was
    /// disabled (see [`BackfillOptions::allow_recompute`]).
    Skipped,
}

/// Options controlling a backfill run. [`Default`] matches the historical offline behavior:
/// recompute missing state, allow indexing right up to the head, and commit in modest batches.
#[derive(Debug, Clone, Copy)]
pub struct BackfillOptions {
    /// When `true`, missing tipset state is recomputed (expensive); when `false`, such tipsets
    /// are skipped and reported. Online backfills default this to `false` to avoid starving sync.
    pub allow_recompute: bool,
    /// When `false`, the walk start is clamped to `head - CHAIN_FINALITY` so that revert-prone
    /// near-head tipsets are not indexed.
    pub allow_near_head: bool,
    /// Number of tipsets to process between commits/checkpoints.
    pub batch_size: usize,
}

impl Default for BackfillOptions {
    fn default() -> Self {
        Self {
            allow_recompute: true,
            allow_near_head: true,
            batch_size: 1000,
        }
    }
}

/// Report returned by [`run_backfill`].
#[derive(Debug, Clone, Copy, Default)]
pub struct BackfillReport {
    pub indexed: u64,
    pub skipped: u64,
    pub cancelled: bool,
}

/// Lock-free counters for backfill progress; the epoch counters are hot on the walk.
#[derive(Default)]
struct BackfillCounters {
    start_epoch: AtomicI64,
    current_epoch: AtomicI64,
    target_epoch: AtomicI64,
    indexed: AtomicI64,
    skipped: AtomicI64,
}

#[derive(Default)]
struct BackfillStatusInner {
    /// `None` while running and before the first run.
    outcome: Option<ChainExportState>,
    error: Option<String>,
    start_time: Option<chrono::DateTime<chrono::Utc>>,
    cancellation_token: Option<CancellationToken>,
    counters: Arc<BackfillCounters>,
}

impl BackfillStatusInner {
    fn is_running(&self) -> bool {
        self.cancellation_token.is_some()
    }
}

/// Status of the in-daemon index backfill, surfaced by the `Forest.IndexBackfillStatus` RPC and
/// driven only through [`BackfillGuard`]. Mirrors the lifecycle of [`ChainExportState`].
#[derive(Default)]
pub struct BackfillStatus {
    inner: parking_lot::Mutex<BackfillStatusInner>,
}

/// A consistent snapshot of [`BackfillStatus`], read under a single lock.
#[derive(Debug, Clone)]
pub struct BackfillStatusSnapshot {
    pub state: ChainExportState,
    pub error: Option<String>,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub start_epoch: ChainEpoch,
    pub current_epoch: ChainEpoch,
    pub target_epoch: ChainEpoch,
    pub indexed: u64,
    pub skipped: u64,
}

impl BackfillStatus {
    pub fn snapshot(&self) -> BackfillStatusSnapshot {
        let inner = self.inner.lock();
        let c = &inner.counters;
        BackfillStatusSnapshot {
            state: if inner.is_running() {
                ChainExportState::Running
            } else {
                inner.outcome.unwrap_or(ChainExportState::Idle)
            },
            error: inner.error.clone(),
            start_time: inner.start_time,
            start_epoch: c.start_epoch.load(Ordering::Relaxed),
            current_epoch: c.current_epoch.load(Ordering::Relaxed),
            target_epoch: c.target_epoch.load(Ordering::Relaxed),
            indexed: c.indexed.load(Ordering::Relaxed).max(0) as u64,
            skipped: c.skipped.load(Ordering::Relaxed).max(0) as u64,
        }
    }

    /// Cancels the running backfill, if any, returning whether one was running.
    pub fn cancel_running(&self) -> bool {
        if let Some(token) = &self.inner.lock().cancellation_token {
            token.cancel();
            true
        } else {
            false
        }
    }

    fn try_begin(
        &self,
        cancellation_token: CancellationToken,
    ) -> anyhow::Result<Arc<BackfillCounters>> {
        let mut inner = self.inner.lock();
        anyhow::ensure!(
            !inner.is_running(),
            "an index backfill is already running; check `forest-cli index backfill --status`",
        );
        let counters = Arc::new(BackfillCounters::default());
        *inner = BackfillStatusInner {
            outcome: None,
            error: None,
            start_time: Some(chrono::Utc::now()),
            cancellation_token: Some(cancellation_token),
            counters: counters.clone(),
        };
        Ok(counters)
    }

    fn record_outcome(&self, outcome: ChainExportState, error: Option<String>) {
        let mut inner = self.inner.lock();
        if inner.outcome.is_none() {
            inner.outcome = Some(outcome);
            inner.error = error;
        }
    }

    fn end(&self) {
        let mut inner = self.inner.lock();
        if inner.outcome.is_none() {
            inner.outcome = Some(ChainExportState::Failed);
        }
        inner.cancellation_token = None;
    }
}

/// Global status of the in-daemon index backfill.
pub static BACKFILL_STATUS: LazyLock<BackfillStatus> = LazyLock::new(BackfillStatus::default);

/// Single-flight guard for an index backfill. Holds the [`BACKFILL_STATUS`] slot for progress and
/// cancellation, and nests a [`ChainExportGuard`] so backfill never overlaps snapshot exports or
/// the snapshot GC (all three share the chain-export slot).
pub struct BackfillGuard {
    cancellation_token: CancellationToken,
    counters: Arc<BackfillCounters>,
    // Held for the lifetime of the backfill to exclude exports and snapshot GC.
    _export_guard: crate::ipld::ChainExportGuard,
}

impl BackfillGuard {
    pub fn try_start() -> anyhow::Result<Self> {
        // Acquire the shared chain-export slot first so backfill excludes GC/exports.
        let export_guard = crate::ipld::ChainExportGuard::try_start_export(
            crate::ipld::ChainExportKind::IndexBackfill,
        )?;
        let cancellation_token = CancellationToken::new();
        let counters = match BACKFILL_STATUS.try_begin(cancellation_token.clone()) {
            Ok(counters) => counters,
            Err(e) => {
                // Roll back the export slot if another backfill is somehow already tracked.
                drop(export_guard);
                return Err(e);
            }
        };
        Ok(Self {
            cancellation_token,
            counters,
            _export_guard: export_guard,
        })
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }

    /// Records the terminal outcome for the backfill.
    pub fn finish<T>(self, result: anyhow::Result<T>) -> anyhow::Result<T> {
        match &result {
            Ok(_) => BACKFILL_STATUS.record_outcome(ChainExportState::Succeeded, None),
            Err(e) => {
                BACKFILL_STATUS.record_outcome(ChainExportState::Failed, Some(format!("{e:#}")))
            }
        }
        result
    }

    fn reset_counters(&self, start_epoch: ChainEpoch, target_epoch: ChainEpoch) {
        self.counters
            .start_epoch
            .store(start_epoch, Ordering::Relaxed);
        self.counters
            .current_epoch
            .store(start_epoch, Ordering::Relaxed);
        self.counters
            .target_epoch
            .store(target_epoch, Ordering::Relaxed);
        self.counters.indexed.store(0, Ordering::Relaxed);
        self.counters.skipped.store(0, Ordering::Relaxed);
    }

    fn set_current(&self, epoch: ChainEpoch) {
        self.counters.current_epoch.store(epoch, Ordering::Relaxed);
    }

    fn inc_indexed(&self) {
        self.counters.indexed.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_skipped(&self) {
        self.counters.skipped.fetch_add(1, Ordering::Relaxed);
    }

    fn record_cancelled(&self) {
        BACKFILL_STATUS.record_outcome(ChainExportState::Cancelled, None);
    }
}

impl Drop for BackfillGuard {
    fn drop(&mut self) {
        self.cancellation_token.cancel();
        BACKFILL_STATUS.end();
    }
}

/// Reads the persisted backfill checkpoint epoch, if any. A cleared checkpoint (see
/// [`clear_backfill_checkpoint`]) is reported as `None`.
pub fn read_backfill_checkpoint(
    state_manager: &StateManager,
) -> anyhow::Result<Option<ChainEpoch>> {
    Ok(state_manager
        .db()
        .read_obj::<ChainEpoch>(BACKFILL_CHECKPOINT_KEY)?
        .filter(|epoch| *epoch != ChainEpoch::MIN))
}

fn write_backfill_checkpoint(
    state_manager: &StateManager,
    epoch: ChainEpoch,
) -> anyhow::Result<()> {
    state_manager
        .db()
        .write_obj(BACKFILL_CHECKPOINT_KEY, &epoch)
}

fn clear_backfill_checkpoint(state_manager: &StateManager) -> anyhow::Result<()> {
    // Persist a sentinel rather than deleting: the settings store has no typed delete here and a
    // stale checkpoint is only used as a resume hint, which the caller validates against the range.
    state_manager
        .db()
        .write_obj(BACKFILL_CHECKPOINT_KEY, &ChainEpoch::MIN)
}

async fn process_ts(
    ts: &Tipset,
    state_manager: &StateManager,
    delegated_messages: &mut Vec<(SignedMessage, u64)>,
    allow_recompute: bool,
) -> anyhow::Result<ProcessOutcome> {
    let epoch = ts.epoch();
    let tsk = ts.key().clone();

    let executed = match state_manager
        .load_executed_tipset_for_backfill(ts, allow_recompute)
        .await
    {
        Ok(executed) => executed,
        // With recomputation allowed, a load failure is a real error. With it disabled, a missing
        // state output (e.g. reclaimed by GC) is expected: skip and report rather than fail.
        Err(e) if allow_recompute => return Err(e),
        Err(e) => {
            tracing::warn!(
                "skipping tipset @{epoch} during backfill (state unavailable, recomputation disabled): {e:#}"
            );
            return Ok(ProcessOutcome::Skipped);
        }
    };
    crate::rpc::eth::store_block_logs_bloom(
        state_manager,
        ts,
        &executed.state_root,
        &executed.executed_messages,
    )?;

    delegated_messages.append(
        &mut state_manager
            .chain_store()
            .headers_delegated_messages(ts.block_headers().iter())?,
    );
    tracing::trace!("Indexing tipset @{}: {}", epoch, &tsk);
    tsk.save(state_manager.db())?;

    Ok(ProcessOutcome::Indexed)
}

#[derive(Clone, Copy)]
pub enum RangeSpec {
    To(ChainEpoch),
    NumTipsets(usize),
}

impl std::fmt::Display for RangeSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RangeSpec::To(epoch) => write!(f, "To epoch:      {}", epoch),
            RangeSpec::NumTipsets(n) => write!(f, "Tipsets:       {}", n),
        }
    }
}

/// To support the Event RPC API, a new column has been added to parity-db to handle the mapping:
/// - Events root [`Cid`] -> [`TipsetKey`].
///
/// Similarly, to support the Ethereum RPC API, another column has been introduced to map:
/// - [`struct@EthHash`] -> [`TipsetKey`],
/// - [`struct@EthHash`] -> Delegated message [`Cid`].
///
/// This function traverses the chain store and populates these columns accordingly. It is a thin
/// wrapper over [`run_backfill`] with the historical (offline) options and no cancellation.
pub async fn backfill_db(
    state_manager: &StateManager,
    head_ts: &Tipset,
    spec: RangeSpec,
) -> anyhow::Result<()> {
    let guard = BackfillGuard::try_start()?;
    let result = run_backfill(
        state_manager,
        head_ts,
        spec,
        BackfillOptions::default(),
        &guard,
    )
    .await;
    let report = guard.finish(result)?;
    tracing::info!(
        "Total successful backfills: {} (skipped: {})",
        report.indexed,
        report.skipped
    );
    Ok(())
}

/// Hardened index backfill core shared by the offline `forest-tool index backfill` command and the
/// online `Forest.IndexBackfill` RPC method.
///
/// Beyond the plain chain walk it:
/// - clamps the start below `CHAIN_FINALITY` unless [`BackfillOptions::allow_near_head`] is set,
/// - commits and checkpoints in batches of [`BackfillOptions::batch_size`] so a large range is not
///   a single transaction and can be resumed,
/// - honors `cancel` between tipsets,
/// - writes Ethereum mappings with newest-wins semantics so it does not clobber the live head
///   indexer, and
/// - re-indexes tipsets applied while the walk was running (revert-awareness).
///
/// Progress is published to [`BACKFILL_STATUS`] via `guard`.
pub async fn run_backfill(
    state_manager: &StateManager,
    from_ts: &Tipset,
    spec: RangeSpec,
    options: BackfillOptions,
    guard: &BackfillGuard,
) -> anyhow::Result<BackfillReport> {
    tracing::info!("Starting index backfill...");

    let cancel = guard.cancellation_token();

    // Subscribe before the walk so applies/reverts that happen during it are observed.
    let mut head_rx = state_manager.chain_store().subscribe_head_changes();

    // Optionally clamp the start below finality to avoid indexing revert-prone near-head tipsets.
    let start_ts = if options.allow_near_head {
        from_ts.shallow_clone()
    } else {
        let head_epoch = state_manager.heaviest_tipset().epoch();
        let safe_epoch = head_epoch.saturating_sub(CHAIN_FINALITY);
        if from_ts.epoch() > safe_epoch {
            state_manager
                .chain_index()
                .load_required_tipset_by_height(
                    safe_epoch,
                    from_ts.shallow_clone(),
                    crate::chain::index::ResolveNullTipset::TakeOlder,
                )
                .await?
        } else {
            from_ts.shallow_clone()
        }
    };

    let target_epoch = match spec {
        RangeSpec::To(to_epoch) => to_epoch,
        // Not known exactly ahead of time; approximate for progress reporting.
        RangeSpec::NumTipsets(n) => start_ts.epoch().saturating_sub(n as ChainEpoch),
    };
    guard.reset_counters(start_ts.epoch(), target_epoch);

    let mut batch: Vec<(SignedMessage, u64)> = vec![];
    let mut report = BackfillReport::default();
    let mut processed_since_flush = 0usize;
    let mut lowest_epoch = start_ts.epoch();

    for (count, ts) in start_ts
        .shallow_clone()
        .chain(&state_manager.chain_store().db())
        .enumerate()
    {
        match spec {
            RangeSpec::To(to_epoch) if ts.epoch() < to_epoch => break,
            RangeSpec::NumTipsets(n) if count >= n => break,
            _ => {}
        }

        if cancel.is_cancelled() {
            report.cancelled = true;
            break;
        }

        guard.set_current(ts.epoch());
        lowest_epoch = ts.epoch();
        match process_ts(&ts, state_manager, &mut batch, options.allow_recompute).await? {
            ProcessOutcome::Indexed => {
                report.indexed += 1;
                guard.inc_indexed();
            }
            ProcessOutcome::Skipped => {
                report.skipped += 1;
                guard.inc_skipped();
            }
        }
        processed_since_flush += 1;

        if processed_since_flush >= options.batch_size {
            state_manager
                .chain_store()
                .process_signed_messages(&batch, true)?;
            batch.clear();
            write_backfill_checkpoint(state_manager, ts.epoch())?;
            processed_since_flush = 0;
        }
    }

    // Final commit of the trailing batch.
    state_manager
        .chain_store()
        .process_signed_messages(&batch, true)?;
    batch.clear();

    // Revert-awareness: re-index tipsets applied while walking so the canonical mapping wins
    // under newest-wins semantics. Only relevant for the range we just covered.
    if !report.cancelled {
        let mut extra: Vec<(SignedMessage, u64)> = vec![];
        loop {
            match head_rx.try_recv() {
                Ok(changes) => {
                    for ts in changes.applies {
                        if ts.epoch() >= lowest_epoch && ts.epoch() <= start_ts.epoch() {
                            tracing::debug!(
                                "re-indexing tipset @{} applied during backfill",
                                ts.epoch()
                            );
                            if let Err(e) =
                                process_ts(&ts, state_manager, &mut extra, options.allow_recompute)
                                    .await
                            {
                                tracing::warn!(
                                    "failed to re-index applied tipset @{}: {e:#}",
                                    ts.epoch()
                                );
                            }
                        }
                    }
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => break,
                Err(TryRecvError::Lagged(n)) => {
                    tracing::warn!("backfill head-change listener lagged: skipped {n} events");
                    continue;
                }
            }
        }
        if !extra.is_empty() {
            state_manager
                .chain_store()
                .process_signed_messages(&extra, true)?;
        }
    }

    if report.cancelled {
        // Persist where we stopped so the run can be resumed, and reflect the cancellation.
        write_backfill_checkpoint(state_manager, lowest_epoch)?;
        guard.record_cancelled();
        tracing::info!(
            "Index backfill cancelled after {} tipsets (skipped: {})",
            report.indexed,
            report.skipped
        );
    } else {
        // Successful completion: clear the resume checkpoint.
        clear_backfill_checkpoint(state_manager)?;
    }

    Ok(report)
}

#[cfg(test)]
mod test {
    use super::*;

    // The backfill guard shares the chain-export single-flight slot, so serialize with the export
    // tests that also touch it.
    #[test]
    #[serial_test::serial(chain_export)]
    fn backfill_guard_is_single_flight_and_records_outcomes() {
        let g = BackfillGuard::try_start().unwrap();
        assert_eq!(BACKFILL_STATUS.snapshot().state, ChainExportState::Running);

        // A second concurrent backfill is rejected while the first is running.
        assert!(BackfillGuard::try_start().is_err());

        // Succeeded is recorded via `finish`.
        g.finish(anyhow::Ok(())).unwrap();
        assert_eq!(
            BACKFILL_STATUS.snapshot().state,
            ChainExportState::Succeeded
        );

        // A new run can start once the previous one finished, and failures are recorded.
        let g = BackfillGuard::try_start().unwrap();
        g.finish(anyhow::Result::<()>::Err(anyhow::anyhow!("boom")))
            .unwrap_err();
        let snapshot = BACKFILL_STATUS.snapshot();
        assert_eq!(snapshot.state, ChainExportState::Failed);
        assert_eq!(snapshot.error.as_deref(), Some("boom"));

        // A guard dropped without `finish` lands in `Failed`.
        let g = BackfillGuard::try_start().unwrap();
        drop(g);
        assert_eq!(BACKFILL_STATUS.snapshot().state, ChainExportState::Failed);
    }

    #[test]
    #[serial_test::serial(chain_export)]
    fn backfill_cancellation_is_observable_and_wins() {
        let g = BackfillGuard::try_start().unwrap();
        // The cancel handler cancels the running backfill.
        assert!(BACKFILL_STATUS.cancel_running());
        assert!(g.cancellation_token().is_cancelled());

        // The cooperative-cancel path records `Cancelled`, and that terminal state wins over a
        // subsequent `finish(Ok(..))` (as happens when the walk returns a cancelled report).
        g.record_cancelled();
        g.finish(anyhow::Ok(())).unwrap();
        assert_eq!(
            BACKFILL_STATUS.snapshot().state,
            ChainExportState::Cancelled
        );

        // With no backfill running, cancel is a no-op.
        assert!(!BACKFILL_STATUS.cancel_running());
    }

    #[tokio::test]
    async fn import_snapshot_from_file_valid() {
        for import_mode in [ImportMode::Auto, ImportMode::Copy, ImportMode::Move] {
            import_snapshot_from_file("test-snapshots/chain4.car", import_mode)
                .await
                .unwrap();
        }

        // Linking is not supported for raw CAR files.
        for import_mode in [ImportMode::Symlink, ImportMode::Hardlink] {
            import_snapshot_from_file("test-snapshots/chain4.car", import_mode)
                .await
                .unwrap_err();
        }
    }

    #[tokio::test]
    async fn import_snapshot_from_compressed_file_valid() {
        for import_mode in [ImportMode::Auto, ImportMode::Copy, ImportMode::Move] {
            import_snapshot_from_file("test-snapshots/chain4.car.zst", import_mode)
                .await
                .unwrap();
        }

        // Linking is not supported for raw CAR files.
        for import_mode in [ImportMode::Symlink, ImportMode::Hardlink] {
            import_snapshot_from_file("test-snapshots/chain4.car", import_mode)
                .await
                .unwrap_err();
        }
    }

    #[tokio::test]
    async fn import_snapshot_from_forest_car_valid() {
        for import_mode in [
            ImportMode::Auto,
            ImportMode::Copy,
            ImportMode::Move,
            ImportMode::Symlink,
            ImportMode::Hardlink,
        ] {
            import_snapshot_from_file("test-snapshots/chain4.forest.car.zst", import_mode)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn import_snapshot_from_file_invalid() {
        for import_mode in &[
            ImportMode::Auto,
            ImportMode::Copy,
            ImportMode::Move,
            ImportMode::Symlink,
            ImportMode::Hardlink,
        ] {
            import_snapshot_from_file("Cargo.toml", *import_mode)
                .await
                .unwrap_err();
        }
    }

    #[tokio::test]
    async fn import_snapshot_from_file_not_found() {
        for import_mode in &[
            ImportMode::Auto,
            ImportMode::Copy,
            ImportMode::Move,
            ImportMode::Symlink,
            ImportMode::Hardlink,
        ] {
            import_snapshot_from_file("dummy.car", *import_mode)
                .await
                .unwrap_err();
        }
    }

    #[tokio::test]
    async fn import_snapshot_from_url_not_found() {
        for import_mode in &[
            ImportMode::Auto,
            ImportMode::Copy,
            ImportMode::Move,
            ImportMode::Symlink,
            ImportMode::Hardlink,
        ] {
            import_snapshot_from_file("https://forest.chainsafe.io/dummy.car", *import_mode)
                .await
                .unwrap_err();
        }
    }

    async fn import_snapshot_from_file(
        file_path: &str,
        import_mode: ImportMode,
    ) -> anyhow::Result<()> {
        // Prevent modifications on the original file, e.g., deletion via `ImportMode::Move`.
        let temp_file = tempfile::Builder::new().tempfile()?;
        fs::copy(Path::new(file_path), temp_file.path())?;
        let file_path = temp_file.path();

        let temp_db_dir = tempfile::Builder::new().tempdir()?;

        let (path, ts) = import_chain_as_forest_car(
            file_path,
            temp_db_dir.path(),
            import_mode,
            "http://127.0.0.1:2345/rpc/v1".parse().unwrap(),
            Path::new("test"),
            &ChainConfig::devnet(),
            &SnapshotProgressTracker::default(),
        )
        .await?;
        match import_mode {
            ImportMode::Symlink => {
                assert_eq!(
                    std::path::absolute(path.read_link()?)?,
                    std::path::absolute(file_path)?
                );
            }
            ImportMode::Move => {
                assert!(!file_path.exists());
                assert!(path.is_file());
            }
            _ => {
                assert!(file_path.is_file());
                assert!(path.is_file());
            }
        }
        assert!(ts.epoch() > 0);
        Ok(())
    }
}
