// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::fetch_snapshot_if_required;
use crate::blocks::Tipset;
use crate::cli_shared::{
    chain_path,
    cli::{CliOpts, Config},
    snapshot,
};
use crate::db::car::forest::FOREST_CAR_FILE_EXTENSION;
use crate::db::car::{ForestCar, ManyCar};
use crate::db::db_engine::{db_root, open_proxy_db};
use crate::db::rolling::RollingDB;
use crate::utils::db::car_stream::CarStream;
use crate::utils::io::Mmap;
use anyhow::{bail, Context};
use futures::TryStreamExt;
use std::ffi::OsStr;
use std::fs;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time,
};
use tokio::io::AsyncWriteExt;
use tracing::info;
use url::Url;
use walkdir::WalkDir;

/// This function tries to open the forest database directory as [`ManyCar<Arc<RollingDB>>`], it
/// 1. loads `parity-db`
/// 2. loads all existing CAR files in `.forest.car.zst` format
/// 3. asks to fetch the latest snapshot when it's required to run forest.
/// 4. imports the snapshot(from either CLI options or step 3) and stores it to the database in `.forest.car.zst` format

pub async fn prepare_and_open_forest_car_union_db(
    config: &mut Config,
    opts: &CliOpts,
) -> anyhow::Result<(Arc<ManyCar<Arc<RollingDB>>>, Option<Tipset>)> {
    let chain_data_path = chain_path(config);
    let db_root_dir = db_root(&chain_data_path);
    let forest_car_db_dir = db_root_dir.join("car_db");
    if !forest_car_db_dir.is_dir() {
        fs::create_dir_all(&forest_car_db_dir)?;
    }

    // Opens parity-db
    let mut store = ManyCar::new(Arc::new(open_proxy_db(
        db_root_dir.clone(),
        config.db_config().clone(),
    )?));

    // Load existing CAR DB(s)
    load_forest_cars(&mut store, &forest_car_db_dir)?;

    // Fetch the latest snapshot if needed
    // TODO: use `--consume-snapshot` CLI option once it's implemented
    let mut consume_snapshot_file = false;
    if config.client.snapshot_path.is_none() {
        fetch_snapshot_if_required(
            &store,
            &store,
            config,
            opts.auto_download_snapshot,
            &db_root_dir,
        )
        .await?;
        consume_snapshot_file = true;
    }

    // Import chain if needed
    let heaviest_tipset = if !opts.skip_load.unwrap_or_default() {
        if let Some(path) = &config.client.snapshot_path {
            let (car_db_path, ts) =
                import_chain_as_forest_car(path, &forest_car_db_dir, consume_snapshot_file).await?;
            store.read_only_files(std::iter::once(car_db_path.clone()))?;
            info!("Loaded car DB at {}", car_db_path.display());
            Some(ts)
        } else {
            None
        }
    } else {
        None
    };

    Ok((Arc::new(store), heaviest_tipset))
}

fn load_forest_cars<T>(
    store: &mut ManyCar<T>,
    forest_car_db_dir: impl AsRef<Path>,
) -> anyhow::Result<()> {
    for file in WalkDir::new(forest_car_db_dir)
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
        match ForestCar::new(Mmap::map_path(&file)?) {
            Ok(car) => {
                store.read_only(car.into());
                info!("Loaded car DB at {}", file.display());
            }
            Err(err) => {
                bail!("Error loading car DB at {}: {err}", file.display())
            }
        };
    }

    Ok(())
}

/// This function validates and stores the CAR binary from `from_path`(either local path or URL) into the `{DB_ROOT}/car_db/`
/// (automatically trans-code into `.forest.car.zst` format when needed), and returns its final file path and the heaviest tipset.
async fn import_chain_as_forest_car(
    from_path: &Path,
    forest_car_db_dir: &Path,
    consume_snapshot_file: bool,
) -> anyhow::Result<(PathBuf, Tipset)> {
    info!("Importing chain from snapshot at: {}", from_path.display());

    let stopwatch = time::Instant::now();

    let downloaded_car_temp_path =
        tempfile::NamedTempFile::new_in(forest_car_db_dir)?.into_temp_path();
    if let Ok(url) = Url::parse(&from_path.display().to_string()) {
        download_to(url, &downloaded_car_temp_path).await?;
    } else {
        move_or_copy_file(from_path, &downloaded_car_temp_path, consume_snapshot_file)?;
    }

    let forest_car_db_path = forest_car_db_dir.join(format!(
        "{}{FOREST_CAR_FILE_EXTENSION}",
        chrono::Utc::now().timestamp_millis()
    ));

    if ForestCar::is_valid(&Mmap::map_path(&downloaded_car_temp_path)?) {
        downloaded_car_temp_path.persist(&forest_car_db_path)?;
    } else {
        // Use another temp file to make sure all final `.forest.car.zst` files are complete and valid.
        let forest_car_db_temp_path =
            tempfile::NamedTempFile::new_in(forest_car_db_dir)?.into_temp_path();
        transcode_into_forest_car(&downloaded_car_temp_path, &forest_car_db_temp_path).await?;
        forest_car_db_temp_path.persist(&forest_car_db_path)?;
    }

    let ts = ForestCar::new(Mmap::map_path(&forest_car_db_path)?)?.heaviest_tipset()?;
    info!(
        "Imported snapshot in: {}s, heaviest tipset epoch: {}",
        stopwatch.elapsed().as_secs(),
        ts.epoch()
    );

    Ok((forest_car_db_path, ts))
}

async fn download_to(url: Url, destination: &Path) -> anyhow::Result<()> {
    snapshot::download_file(
        url,
        destination
            .parent()
            .context("Infallible getting the directory")?,
        destination
            .file_name()
            .and_then(OsStr::to_str)
            .context("Infallible getting the file name")?,
    )
    .await?;

    Ok(())
}

fn move_or_copy_file(from: &Path, to: &Path, prefer_move: bool) -> anyhow::Result<()> {
    if prefer_move && fs::rename(from, to).is_ok() {
        return Ok(());
    } else {
        fs::copy(from, to)?;
    }

    Ok(())
}

async fn transcode_into_forest_car(from: &Path, to: &Path) -> anyhow::Result<()> {
    let car_stream = CarStream::new(tokio::io::BufReader::new(
        tokio::fs::File::open(from).await?,
    ))
    .await?;
    let roots = car_stream.header.roots.clone();

    let mut writer = tokio::io::BufWriter::new(tokio::fs::File::create(to).await?);
    let frames = crate::db::car::forest::Encoder::compress_stream(
        8000usize.next_power_of_two(),
        zstd::DEFAULT_COMPRESSION_LEVEL as _,
        car_stream.map_err(anyhow::Error::from),
    );
    crate::db::car::forest::Encoder::write(&mut writer, roots, frames).await?;
    writer.shutdown().await?;

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn import_snapshot_from_file_valid() {
        import_snapshot_from_file("test-snapshots/chain4.car")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn import_snapshot_from_compressed_file_valid() {
        import_snapshot_from_file("test-snapshots/chain4.car.zst")
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn import_snapshot_from_file_invalid() {
        import_snapshot_from_file("Cargo.toml").await.unwrap_err();
    }

    #[tokio::test]
    async fn import_snapshot_from_file_not_found() {
        import_snapshot_from_file("dummy.car").await.unwrap_err();
    }

    #[tokio::test]
    async fn import_snapshot_from_url_not_found() {
        import_snapshot_from_file("https://dummy.com/dummy.car")
            .await
            .unwrap_err();
    }

    async fn import_snapshot_from_file(file_path: &str) -> anyhow::Result<()> {
        let temp = tempfile::Builder::new().tempdir()?;
        let (path, ts) =
            import_chain_as_forest_car(Path::new(file_path), temp.path(), false).await?;
        assert!(path.is_file());
        assert!(ts.epoch() > 0);
        Ok(())
    }
}
