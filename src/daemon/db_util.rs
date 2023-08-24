// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::{Tipset, TipsetKeys};
use crate::cli_shared::{
    chain_path,
    cli::{CliOpts, Config},
    snapshot,
};
use crate::daemon::asyncify;
use crate::db::car::forest::FOREST_CAR_FILE_EXTENSION;
use crate::db::car::{ForestCar, ManyCar};
use crate::db::db_engine::{db_root, open_proxy_db};
use crate::db::rolling::RollingDB;
use crate::db::SettingsStore;
use crate::genesis::read_genesis_header;
use crate::ipld::FrozenCids;
use crate::shim::version::NetworkVersion;
use crate::utils::db::car_stream::CarStream;
use crate::utils::{retry, RetryArgs};
use anyhow::{bail, Context};
use chrono::Utc;
use dialoguer::theme::ColorfulTheme;
use futures::TryStreamExt;
use fvm_ipld_blockstore::Blockstore;
use positioned_io::RandomAccessFile;
use std::ffi::OsStr;
use std::fs;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time,
    time::Duration,
};
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};
use url::Url;
use walkdir::WalkDir;

/// This function tries to open the forest database directory as [`ManyCar<Arc<RollingDB>>`], it
/// 1. loads `parity-db`
/// 2. loads all existing CAR files in `.forest.car.zst` format
/// 3. asks to fetch the latest snapshot when it's required to run forest.
/// 4. imports the snapshot(from either CLI options or step 3) and stores it to the database in `.forest.car.zst` format

pub async fn open_forest_car_union_db(
    config: &mut Config,
    opts: &CliOpts,
) -> anyhow::Result<(Arc<ManyCar<Arc<RollingDB>>>, Option<Tipset>)> {
    let mut heaviest_tipset: Option<Tipset> = None;
    let chain_data_path = chain_path(config);
    let db_root_dir = db_root(&chain_data_path);
    let forest_car_db_dir = db_root_dir.join("car_db");
    if !forest_car_db_dir.is_dir() {
        fs::create_dir_all(&forest_car_db_dir)?;
    }

    let mut store = ManyCar::new(Arc::new(open_proxy_db(
        db_root_dir.clone(),
        config.db_config().clone(),
    )?));

    // Load existing CAR DB(s)
    for file in WalkDir::new(&forest_car_db_dir)
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
        match ForestCar::new(RandomAccessFile::open(&file)?) {
            Ok(car) => {
                store.read_only(car.into());
                info!("Loaded car DB at {}", file.display());
            }
            Err(err) => {
                bail!("Error loading car DB at {}: {err}", file.display())
            }
        };
    }

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

    if !opts.skip_load.unwrap_or_default() {
        if let Some(path) = &config.client.snapshot_path {
            let (car_db_path, ts) =
                import_chain_as_forest_car(path, &forest_car_db_dir, consume_snapshot_file).await?;
            heaviest_tipset = Some(ts);
            store.read_only_files(std::iter::once(car_db_path.clone()))?;
            info!("Loaded car DB at {}", car_db_path.display());
        }
    }

    Ok((Arc::new(store), heaviest_tipset))
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
    let temp_file_ready = if from_path.is_file() && consume_snapshot_file {
        if let Err(err) = fs::rename(from_path, &downloaded_car_temp_path) {
            warn!(
                "Failed to rename file from {} to {}: {err}",
                from_path.display(),
                downloaded_car_temp_path.display()
            );
            false
        } else {
            true
        }
    } else {
        false
    };

    if !temp_file_ready {
        if from_path.is_file() {
            std::fs::copy(from_path, &downloaded_car_temp_path)?;
        } else {
            let url = Url::parse(&from_path.display().to_string())?;
            snapshot::download_file(
                url,
                forest_car_db_dir,
                downloaded_car_temp_path
                    .file_name()
                    .and_then(OsStr::to_str)
                    .context("Infallible getting file name")?,
            )
            .await?;
        }
    }

    let forest_car_db_path = forest_car_db_dir.join(format!(
        "{}{FOREST_CAR_FILE_EXTENSION}",
        chrono::Utc::now().timestamp()
    ));

    let forest_car = ForestCar::new(RandomAccessFile::open(&downloaded_car_temp_path)?);
    let ts = if let Ok(car) = forest_car {
        let ts = car.heaviest_tipset()?;
        downloaded_car_temp_path.persist(&forest_car_db_path)?;
        ts
    } else {
        let car_stream = CarStream::new(tokio::io::BufReader::new(
            tokio::fs::File::open(&downloaded_car_temp_path).await?,
        ))
        .await?;
        let roots = car_stream.header.roots.clone();
        // Use another temp file to make sure all final `.forest.car.zst` files are complete and valid.
        let forest_car_db_temp_path =
            tempfile::NamedTempFile::new_in(forest_car_db_dir)?.into_temp_path();
        {
            let mut writer =
                tokio::io::BufWriter::new(tokio::fs::File::create(&forest_car_db_temp_path).await?);
            let frames = crate::db::car::forest::Encoder::compress_stream(
                8000usize.next_power_of_two(),
                zstd::DEFAULT_COMPRESSION_LEVEL as _,
                car_stream.map_err(anyhow::Error::from),
            );
            crate::db::car::forest::Encoder::write(&mut writer, roots, frames).await?;
            writer.shutdown().await?;
        }
        let ts =
            ForestCar::new(RandomAccessFile::open(&forest_car_db_temp_path)?)?.heaviest_tipset()?;
        forest_car_db_temp_path.persist(&forest_car_db_path)?;
        ts
    };

    info!(
        "Imported snapshot in: {}s, tipset epoch: {}",
        stopwatch.elapsed().as_secs(),
        ts.epoch()
    );

    Ok((forest_car_db_path, ts))
}

/// If our current chain is below a supported height, we need a snapshot to bring it up
/// to a supported height. If we've not been given a snapshot by the user, get one.
///
/// An [`Err`] should be considered fatal.
async fn fetch_snapshot_if_required(
    store: &impl Blockstore,
    settings: &impl SettingsStore,
    config: &mut Config,
    auto_download_snapshot: bool,
    download_directory: &Path,
) -> anyhow::Result<()> {
    if !download_directory.is_dir() {
        anyhow::bail!(
            "`download_directory` does not exist: {}",
            download_directory.display()
        );
    }

    let vendor = snapshot::TrustedVendor::default();
    let chain = &config.chain.network;

    let require_a_snapshot = {
        if let Ok(Some(ts)) = Tipset::load_heaviest(store, settings) {
            let epoch = ts.epoch();
            if verify_tipsets_integrity(store, ts, config).await? {
                // What height is our chain at right now, and what network version does that correspond to?
                let network_version = config.chain.network_version(epoch);
                // We don't support small network versions (we can't validate from e.g genesis).
                // So we need a snapshot (which will be from a recent network version)
                network_version < NetworkVersion::V16
            } else {
                true
            }
        } else {
            true
        }
    };

    let have_a_snapshot = config.client.snapshot_path.is_some();

    match (require_a_snapshot, have_a_snapshot, auto_download_snapshot) {
        (false, _, _) => Ok(()),   // noop - don't need a snapshot
        (true, true, _) => Ok(()), // noop - we need a snapshot, and we have one
        (true, false, true) => {
            // we need a snapshot, don't have one, and have permission to download one, so do that
            let max_retries = 3;
            match retry(
                RetryArgs {
                    timeout: None,
                    max_retries: Some(max_retries),
                    delay: Some(Duration::from_secs(60)),
                },
                || crate::cli_shared::snapshot::fetch(download_directory, chain, vendor),
            )
            .await
            {
                Ok(path) => {
                    config.client.snapshot_path = Some(path);
                    config.client.snapshot = true;
                    Ok(())
                }
                Err(_) => bail!("failed to fetch snapshot after {max_retries} attempts"),
            }
        }
        (true, false, false) => {
            // we need a snapshot, don't have one, and don't have permission to download one, so ask the user
            let (num_bytes, _url) =
                crate::cli_shared::snapshot::peek(vendor, &config.chain.network)
                    .await
                    .context("couldn't get snapshot size")?;
            // dialoguer will double-print long lines, so manually print the first clause ourselves,
            // then let `Confirm` handle the second.
            println!("Forest requires a snapshot to sync with the network, but automatic fetching is disabled.");
            let message = format!(
                "Fetch a {} snapshot to the current directory? (denying will exit the program). ",
                indicatif::HumanBytes(num_bytes)
            );
            let have_permission = asyncify(|| {
                dialoguer::Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(message)
                    .default(false)
                    .interact()
                    // e.g not a tty (or some other error), so haven't got permission.
                    .unwrap_or(false)
            })
            .await;
            if !have_permission {
                bail!("Forest requires a snapshot to sync with the network, but automatic fetching is disabled.");
            }
            match crate::cli_shared::snapshot::fetch(download_directory, chain, vendor).await {
                Ok(path) => {
                    config.client.snapshot_path = Some(path);
                    config.client.snapshot = true;
                    Ok(())
                }
                Err(e) => Err(e).context("downloading required snapshot failed"),
            }
        }
    }
}

async fn verify_tipsets_integrity(
    store: &impl Blockstore,
    from: Tipset,
    config: &Config,
) -> anyhow::Result<bool> {
    let start = Utc::now();
    info!("Verifying database integrity...");
    let is_valid = if let Some(oldest_ts) = from.chain(store).last() {
        if let Some(genesis_bytes) = config.chain.genesis_bytes() {
            let genesis_header = read_genesis_header(
                config.client.genesis_file.as_ref(),
                Some(genesis_bytes),
                store,
            )
            .await?;
            if let Some(genesis_ts) = Tipset::load(
                store,
                &TipsetKeys::new(FrozenCids::from_iter([*genesis_header.cid()])),
            )? {
                oldest_ts == genesis_ts
            } else {
                false
            }
        } else {
            // For devnet where `config.chain.genesis_bytes()` returns None
            oldest_ts.epoch() == 0
        }
    } else {
        false
    };

    if !is_valid {
        warn!("Failed to validate all tipsets back to genesis, database is likely corrupted.");
    }

    info!(
        "Done verifying database integrity, took {}s",
        (Utc::now() - start).num_seconds()
    );

    Ok(is_valid)
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
