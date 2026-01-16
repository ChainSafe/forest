// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Tipset;
use crate::db::car::forest::{
    FOREST_CAR_FILE_EXTENSION, TEMP_FOREST_CAR_FILE_EXTENSION, new_forest_car_temp_path_in,
};
use crate::db::car::{ForestCar, ManyCar};
use crate::interpreter::VMTrace;
use crate::networks::ChainConfig;
use crate::rpc::sync::SnapshotProgressTracker;
use crate::shim::clock::ChainEpoch;
use crate::state_manager::{NO_CALLBACK, StateManager};
use crate::utils::db::car_stream::CarStream;
use crate::utils::io::EitherMmapOrRandomAccessFile;
use crate::utils::net::{DownloadFileOption, download_to};
use anyhow::{Context, bail};
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time,
};
use tokio::io::AsyncWriteExt;
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

    if let Some(f3_cid) = forest_car.metadata().as_ref().and_then(|m| m.f3_data) {
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
                tracing::error!("Failed to import F3 snapshot: {e}");
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

async fn process_ts<DB>(
    ts: &Tipset,
    state_manager: &Arc<StateManager<DB>>,
    delegated_messages: &mut Vec<(crate::message::SignedMessage, u64)>,
) -> anyhow::Result<()>
where
    DB: fvm_ipld_blockstore::Blockstore + Send + Sync + 'static,
{
    let epoch = ts.epoch();
    let tsk = ts.key().clone();

    state_manager
        .compute_tipset_state(ts.clone(), NO_CALLBACK, VMTrace::NotTraced)
        .await?;

    delegated_messages.append(
        &mut state_manager
            .chain_store()
            .headers_delegated_messages(ts.block_headers().iter())?,
    );
    tracing::trace!("Indexing tipset @{}: {}", epoch, &tsk);
    state_manager.chain_store().put_tipset_key(&tsk)?;

    Ok(())
}

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
/// This function traverses the chain store and populates these columns accordingly.
pub async fn backfill_db<DB>(
    state_manager: &Arc<StateManager<DB>>,
    head_ts: &Tipset,
    spec: RangeSpec,
) -> anyhow::Result<()>
where
    DB: fvm_ipld_blockstore::Blockstore + Send + Sync + 'static,
{
    tracing::info!("Starting index backfill...");

    let mut delegated_messages = vec![];

    let mut num_backfills = 0;

    match spec {
        RangeSpec::To(to_epoch) => {
            for ts in head_ts
                .clone()
                .chain(&state_manager.chain_store().blockstore())
                .take_while(|ts| ts.epoch() >= to_epoch)
            {
                process_ts(&ts, state_manager, &mut delegated_messages).await?;
                num_backfills += 1;
            }
        }
        RangeSpec::NumTipsets(n_tipsets) => {
            for ts in head_ts
                .clone()
                .chain(&state_manager.chain_store().blockstore())
                .take(n_tipsets)
            {
                process_ts(&ts, state_manager, &mut delegated_messages).await?;
                num_backfills += 1;
            }
        }
    }

    state_manager
        .chain_store()
        .process_signed_messages(&delegated_messages)?;

    tracing::info!("Total successful backfills: {}", num_backfills);

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

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
