// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! This module contains the logic for fetching the proofs parameters from the network.
//! As a general rule, the parameters are first fetched from ChainSafe's Cloudflare R2 bucket, if
//! that fails (or is overridden by [`PROOFS_ONLY_IPFS_GATEWAY_ENV`]), the IPFS gateway is used as a fallback.
//!
//! The reason for this is that the IPFS gateway is not as reliable and performant as the centralized solution, which contributed to
//! issues in CI in the past.

use std::{
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

use crate::{
    shim::sector::SectorSize,
    utils::{
        misc::env::is_env_truthy,
        net::{download_ipfs_file_trustlessly, global_http_client},
    },
};
use anyhow::{Context, bail};
use backon::{ExponentialBuilder, Retryable};
use futures::{AsyncWriteExt, TryStreamExt, stream::FuturesUnordered};
use tokio::{
    fs::{self},
    sync::Mutex,
};
use tracing::{debug, info, warn};

use super::parameters::{
    DEFAULT_PARAMETERS, PROOFS_PARAMETER_CACHE_ENV, ParameterData, ParameterMap,
    check_parameter_file, param_dir,
};

/// Default IPFS gateway to use for fetching parameters.
/// Set via the [`IPFS_GATEWAY_ENV`] environment variable.
const DEFAULT_IPFS_GATEWAY: &str = "https://proofs.filecoin.io/ipfs/";
/// Domain bound to the Cloudflare R2 bucket.
const CLOUDFLARE_PROOF_PARAMETER_DOMAIN: &str = "filecoin-proof-parameters.chainsafe.dev";

/// If set to 1, enforce using the IPFS gateway for fetching parameters.
const PROOFS_ONLY_IPFS_GATEWAY_ENV: &str = "FOREST_PROOFS_ONLY_IPFS_GATEWAY";

/// Running Forest requires the download of chain's proof parameters which are large files, by default are hosted outside of China and very slow to download there.
/// To get around that, users should set this variable to:
/// <https://proof-parameters.s3.cn-south-1.jdcloud-oss.com/ipfs/>
const IPFS_GATEWAY_ENV: &str = "IPFS_GATEWAY";

/// Sector size options for fetching.
pub enum SectorSizeOpt {
    /// All keys and proofs gen parameters
    All,
    /// Only verification parameters
    Keys,
    /// All keys and proofs gen parameters for a given size
    Size(SectorSize),
}

/// Ensures the parameter files are downloaded to cache dir
pub async fn ensure_proof_params_downloaded() -> anyhow::Result<()> {
    #[cfg(test)]
    if is_env_truthy("FOREST_TEST_SKIP_PROOF_PARAM_CHECK") {
        return Ok(());
    }

    let data_dir = std::env::var(PROOFS_PARAMETER_CACHE_ENV).unwrap_or_default();
    if data_dir.is_empty() {
        anyhow::bail!("Proof parameter data dir is not set");
    }
    static RUN_ONCE: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));
    let mut run_once = RUN_ONCE.lock().await;
    if *run_once {
        Ok(())
    } else {
        get_params_default(Path::new(&data_dir), SectorSizeOpt::Keys, false).await?;
        *run_once = true;
        Ok(())
    }
}

/// Get proofs parameters and all verification keys for a given sector size
/// given a parameter JSON manifest.
pub async fn get_params(
    data_dir: &Path,
    param_json: &str,
    storage_size: SectorSizeOpt,
    dry_run: bool,
) -> Result<(), anyhow::Error> {
    // Just print out the parameters download directory path and exit.
    if dry_run {
        println!("{}", param_dir(data_dir).to_string_lossy());
        return Ok(());
    }

    fs::create_dir_all(param_dir(data_dir)).await?;

    let params: ParameterMap = serde_json::from_str(param_json)?;

    FuturesUnordered::from_iter(
        params
            .into_iter()
            .filter(|(name, info)| match storage_size {
                SectorSizeOpt::Keys => !name.ends_with("params"),
                SectorSizeOpt::Size(size) => {
                    size as u64 == info.sector_size || !name.ends_with(".params")
                }
                SectorSizeOpt::All => true,
            })
            .map(|(name, info)| async move {
                let data_dir_clone = data_dir.to_owned();
                fetch_verify_params(&data_dir_clone, &name, Arc::new(info)).await
            }),
    )
    .try_collect::<Vec<_>>()
    .await?;

    Ok(())
}

/// Get proofs parameters and all verification keys for a given sector size
/// using default manifest.
#[inline]
pub async fn get_params_default(
    data_dir: &Path,
    storage_size: SectorSizeOpt,
    dry_run: bool,
) -> Result<(), anyhow::Error> {
    get_params(data_dir, DEFAULT_PARAMETERS, storage_size, dry_run).await
}

async fn fetch_verify_params(
    data_dir: &Path,
    name: &str,
    info: Arc<ParameterData>,
) -> Result<(), anyhow::Error> {
    let path: PathBuf = param_dir(data_dir).join(name);

    match check_parameter_file(&path, &info).await {
        Ok(()) => return Ok(()),
        Err(e) => {
            if let Some(e) = e.downcast_ref::<io::Error>() {
                if e.kind() == ErrorKind::NotFound {
                    // File is missing, download it
                }
            } else {
                warn!("Error checking file: {e:?}");
            }
        }
    }

    if is_env_truthy(PROOFS_ONLY_IPFS_GATEWAY_ENV) {
        fetch_params_ipfs_gateway(&path, &info).await?;
    } else if let Err(e) = fetch_params_cloudflare(name, &path).await {
        warn!("Failed to fetch param file from Cloudflare R2: {e:?}. Falling back to IPFS gateway",);
        fetch_params_ipfs_gateway(&path, &info).await?;
    }

    check_parameter_file(&path, &info).await?;
    Ok(())
}

async fn fetch_params_ipfs_gateway(path: &Path, info: &ParameterData) -> anyhow::Result<()> {
    let gateway = std::env::var(IPFS_GATEWAY_ENV)
        .unwrap_or_else(|_| DEFAULT_IPFS_GATEWAY.to_owned())
        .parse()?;
    info!(
        "Fetching param file {path} from {gateway}",
        path = path.display()
    );
    let result = (|| download_ipfs_file_trustlessly(&info.cid, &gateway, path))
        .retry(ExponentialBuilder::default())
        .notify(|err, dur| {
            debug!(
                "retrying download_ipfs_file_trustlessly {err} after {}",
                humantime::format_duration(dur)
            );
        })
        .await;

    debug!(
        "Done fetching param file {path} from {gateway}",
        path = path.display(),
    );
    result
}

/// Downloads the parameter file from Cloudflare R2 to the given path. It wraps the [`download_from_cloudflare`] function with a retry and timeout mechanisms.
async fn fetch_params_cloudflare(name: &str, path: &Path) -> anyhow::Result<()> {
    info!("Fetching param file {name} from Cloudflare R2 {CLOUDFLARE_PROOF_PARAMETER_DOMAIN}");
    let result = (|| download_from_cloudflare(name, path))
        .retry(ExponentialBuilder::default())
        .notify(|err, dur| {
            debug!(
                "retrying download_from_cloudflare {err} after {}",
                humantime::format_duration(dur)
            );
        })
        .await;
    debug!(
        "Done fetching param file {} from Cloudflare",
        path.display()
    );
    result
}

/// Downloads the parameter file from Cloudflare R2 to the given path. In case of an error,
/// the file is not written to the final path to avoid corrupted files.
async fn download_from_cloudflare(name: &str, path: &Path) -> anyhow::Result<()> {
    let response = global_http_client()
        .get(format!(
            "https://{CLOUDFLARE_PROOF_PARAMETER_DOMAIN}/{name}"
        ))
        .send()
        .await
        .context("Failed to fetch param file from Cloudflare R2")?;

    if !response.status().is_success() {
        bail!(
            "Failed to fetch param file from Cloudflare R2: {:?}",
            response
        );
    }
    // Create a temporary file to write the response to. This is to avoid writing
    // to the final file path in case of an error and ending up with corrupted files.
    //
    // Note that we're using the same directory as the final path to avoid moving the file
    // across filesystems.
    let tmp = tempfile::NamedTempFile::new_in(path.parent().context("No parent dir")?)
        .context("Failed to create temp file")?
        .into_temp_path();

    let reader = response
        .bytes_stream()
        .map_err(std::io::Error::other)
        .into_async_read();

    let mut writer = futures::io::BufWriter::new(async_fs::File::create(&tmp).await?);
    futures::io::copy(reader, &mut writer)
        .await
        .context("Failed to write to temp file")?;

    writer.flush().await.context("Failed to flush temp file")?;
    writer.close().await.context("Failed to close temp file")?;

    tmp.persist(path).context("Failed to persist temp file")?;
    Ok(())
}
