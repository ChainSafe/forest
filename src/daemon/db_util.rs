// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Tipset;
use crate::cli_shared::snapshot;
use crate::db::car::forest::FOREST_CAR_FILE_EXTENSION;
use crate::db::car::{ForestCar, ManyCar};
use crate::networks::Height;
use crate::state_manager::StateManager;
use crate::utils::db::car_stream::CarStream;
use crate::utils::io::EitherMmapOrRandomAccessFile;
use anyhow::{bail, Context};
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs;
use std::{
    path::{Path, PathBuf},
    time,
};
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};
use url::Url;
use walkdir::WalkDir;

#[cfg(doc)]
use crate::rpc::eth::Hash;

#[cfg(doc)]
use crate::blocks::TipsetKey;

#[cfg(doc)]
use cid::Cid;

pub fn load_all_forest_cars<T>(store: &ManyCar<T>, forest_car_db_dir: &Path) -> anyhow::Result<()> {
    if !forest_car_db_dir.is_dir() {
        fs::create_dir_all(forest_car_db_dir)?;
    }
    for file in WalkDir::new(forest_car_db_dir)
        .max_depth(1)
        .into_iter()
        .filter_map(|entry| {
            if let Ok(entry) = entry {
                if let Some(filename) = entry.file_name().to_str() {
                    if filename.ends_with(FOREST_CAR_FILE_EXTENSION) {
                        return Some(entry.into_path());
                    }
                }
            }
            None
        })
    {
        let car = ForestCar::try_from(file.as_path())
            .with_context(|| format!("Error loading car DB at {}", file.display()))?;
        store.read_only(car.into())?;
        debug!("Loaded car DB at {}", file.display());
    }

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
    /// Copies the snapshot to the database directory.
    Copy,
    /// Moves the snapshot to the database directory (or copies and deletes the original).
    Move,
    /// Creates a symbolic link to the snapshot in the database directory.
    Link,
}

/// This function validates and stores the CAR binary from `from_path`(either local path or URL) into the `{DB_ROOT}/car_db/`
/// (automatically trans-code into `.forest.car.zst` format when needed), and returns its final file path and the heaviest tipset.
pub async fn import_chain_as_forest_car(
    from_path: &Path,
    forest_car_db_dir: &Path,
    import_mode: ImportMode,
) -> anyhow::Result<(PathBuf, Tipset)> {
    info!("Importing chain from snapshot at: {}", from_path.display());

    let stopwatch = time::Instant::now();

    let forest_car_db_path = forest_car_db_dir.join(format!(
        "{}{FOREST_CAR_FILE_EXTENSION}",
        chrono::Utc::now().timestamp_millis()
    ));

    match import_mode {
        ImportMode::Copy | ImportMode::Move => {
            let downloaded_car_temp_path =
                tempfile::NamedTempFile::new_in(forest_car_db_dir)?.into_temp_path();
            if let Ok(url) = Url::parse(&from_path.display().to_string()) {
                download_to(&url, &downloaded_car_temp_path).await?;
            } else {
                move_or_copy_file(from_path, &downloaded_car_temp_path, import_mode)?;
            }

            if ForestCar::is_valid(&EitherMmapOrRandomAccessFile::open(
                &downloaded_car_temp_path,
            )?) {
                downloaded_car_temp_path.persist(&forest_car_db_path)?;
            } else {
                // Use another temp file to make sure all final `.forest.car.zst` files are complete and valid.
                let forest_car_db_temp_path =
                    tempfile::NamedTempFile::new_in(forest_car_db_dir)?.into_temp_path();
                transcode_into_forest_car(&downloaded_car_temp_path, &forest_car_db_temp_path)
                    .await?;
                forest_car_db_temp_path.persist(&forest_car_db_path)?;
            }
        }
        ImportMode::Link => {
            let from_path = std::path::absolute(from_path)?;
            if ForestCar::is_valid(&EitherMmapOrRandomAccessFile::open(&from_path)?) {
                std::os::unix::fs::symlink(from_path, &forest_car_db_path)
                    .context("Error creating symlink")?;
            } else {
                bail!("Snapshot file must be a valid forest.car.zst file");
            }
        }
    };

    let ts = ForestCar::try_from(forest_car_db_path.as_path())?.heaviest_tipset()?;
    info!(
        "Imported snapshot in: {}s, heaviest tipset epoch: {}",
        stopwatch.elapsed().as_secs(),
        ts.epoch()
    );

    Ok((forest_car_db_path, ts))
}

pub async fn download_to(url: &Url, destination: &Path) -> anyhow::Result<()> {
    snapshot::download_file_with_retry(
        url,
        destination.parent().with_context(|| {
            format!(
                "Error getting the parent directory of {}",
                destination.display()
            )
        })?,
        destination
            .file_name()
            .and_then(OsStr::to_str)
            .with_context(|| format!("Error getting the file name of {}", destination.display()))?,
    )
    .await?;

    Ok(())
}

fn move_or_copy_file(from: &Path, to: &Path, import_mode: ImportMode) -> anyhow::Result<()> {
    match import_mode {
        ImportMode::Move => {
            if fs::rename(from, to).is_ok() {
                Ok(())
            } else {
                fs::copy(from, to).context("Error copying file")?;
                fs::remove_file(from).context("Error removing original file")?;
                Ok(())
            }
        }
        ImportMode::Copy => fs::copy(from, to).map(|_| ()).context("Error copying file"),
        ImportMode::Link => {
            bail!("Linking must be handled elsewhere");
        }
    }
}

async fn transcode_into_forest_car(from: &Path, to: &Path) -> anyhow::Result<()> {
    let car_stream = CarStream::new(tokio::io::BufReader::new(
        tokio::fs::File::open(from).await?,
    ))
    .await?;
    let roots = car_stream.header.roots.clone();

    let mut writer = tokio::io::BufWriter::new(tokio::fs::File::create(to).await?);
    let frames = crate::db::car::forest::Encoder::compress_stream_default(
        car_stream.map_err(anyhow::Error::from),
    );
    crate::db::car::forest::Encoder::write(&mut writer, roots, frames).await?;
    writer.shutdown().await?;

    Ok(())
}

/// For the need for Ethereum RPC API, a new column in parity-db has been introduced to handle
/// mapping of:
/// - [`struct@Hash`] to [`TipsetKey`].
/// - [`struct@Hash`] to delegated message [`Cid`].
///
/// This function traverses the chain store and populates the column.
pub fn populate_eth_mappings<DB>(
    state_manager: &StateManager<DB>,
    head_ts: &Tipset,
) -> anyhow::Result<()>
where
    DB: fvm_ipld_blockstore::Blockstore,
{
    let mut delegated_messages = vec![];

    let hygge = state_manager.chain_config().epoch(Height::Hygge);
    tracing::info!(
        "Populating column EthMappings from range: [{}, {}]",
        hygge,
        head_ts.epoch()
    );

    for ts in head_ts
        .clone()
        .chain(&state_manager.chain_store().blockstore())
    {
        // Hygge is the start of Ethereum support in the FVM (through the FEVM actor).
        // Before this height, no notion of an Ethereum-like API existed.
        if ts.epoch() < hygge {
            break;
        }
        delegated_messages.append(
            &mut state_manager
                .chain_store()
                .headers_delegated_messages(ts.block_headers().iter())?,
        );
        state_manager.chain_store().put_tipset_key(ts.key())?;
    }
    state_manager
        .chain_store()
        .process_signed_messages(&delegated_messages)?;

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn import_snapshot_from_file_valid() {
        for import_mode in &[ImportMode::Copy, ImportMode::Move] {
            import_snapshot_from_file("test-snapshots/chain4.car", *import_mode)
                .await
                .unwrap();
        }

        // Linking is not supported for raw CAR files.
        import_snapshot_from_file("test-snapshots/chain4.car", ImportMode::Link)
            .await
            .unwrap_err();
    }

    #[tokio::test]
    async fn import_snapshot_from_compressed_file_valid() {
        for import_mode in &[ImportMode::Copy, ImportMode::Move] {
            import_snapshot_from_file("test-snapshots/chain4.car.zst", *import_mode)
                .await
                .unwrap();
        }

        // Linking is supported only for `forest.car.zst` files.
        import_snapshot_from_file("test-snapshots/chain4.car.zst", ImportMode::Link)
            .await
            .unwrap_err();
    }

    #[tokio::test]
    async fn import_snapshot_from_forest_car_valid() {
        for import_mode in &[ImportMode::Copy, ImportMode::Move, ImportMode::Link] {
            import_snapshot_from_file("test-snapshots/chain4.forest.car.zst", *import_mode)
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn import_snapshot_from_file_invalid() {
        for import_mode in &[ImportMode::Copy, ImportMode::Move, ImportMode::Link] {
            import_snapshot_from_file("Cargo.toml", *import_mode)
                .await
                .unwrap_err();
        }
    }

    #[tokio::test]
    async fn import_snapshot_from_file_not_found() {
        for import_mode in &[ImportMode::Copy, ImportMode::Move, ImportMode::Link] {
            import_snapshot_from_file("dummy.car", *import_mode)
                .await
                .unwrap_err();
        }
    }

    #[tokio::test]
    async fn import_snapshot_from_url_not_found() {
        for import_mode in &[ImportMode::Copy, ImportMode::Move, ImportMode::Link] {
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
        let (path, ts) =
            import_chain_as_forest_car(file_path, temp_db_dir.path(), import_mode).await?;
        match import_mode {
            ImportMode::Link => {
                assert_eq!(
                    std::path::absolute(path.read_link()?)?,
                    std::path::absolute(file_path)?
                );
            }
            _ => {
                assert!(path.is_file());
            }
        }
        assert!(ts.epoch() > 0);
        Ok(())
    }
}
