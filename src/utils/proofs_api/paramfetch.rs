// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! This module contains the logic for fetching the proofs parameters from the network.
//! Every source is addressed by the parameter file CID, so the file name is only used for the local cache.
//!
//! ChainSafe's mirror is tried before the IPFS gateway, which is neither as reliable nor as performant
//! as the centralized solution and contributed to issues in CI in the past.

use std::{
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

use crate::{
    shim::sector::SectorSize,
    utils::net::{DownloadFileOption, download_ipfs_file_trustlessly, download_to},
};
use anyhow::Context as _;
use backon::{ExponentialBuilder, Retryable};
use cid::Cid;
use futures::{TryStreamExt, stream::FuturesUnordered};
use tokio::{fs, sync::Mutex};
use tracing::{debug, info, warn};
use url::Url;

use super::parameters::{
    DEFAULT_PARAMETERS, PROOFS_PARAMETER_CACHE_ENV, ParameterData, ParameterMap,
    check_parameter_file, param_dir,
};

/// Default IPFS gateway to use for fetching parameters.
/// Set via the [`IPFS_GATEWAY_ENV`] environment variable.
static DEFAULT_IPFS_GATEWAY: LazyLock<Url> = LazyLock::new(|| {
    Url::parse("https://proofs.filecoin.io/ipfs/").expect("invalid default IPFS gateway")
});
static CHAINSAFE_PROOF_PARAMETER_GATEWAY: LazyLock<Url> = LazyLock::new(|| {
    Url::parse("https://filecoin-proofs.chainsafe.dev/ipfs/")
        .expect("invalid ChainSafe proof parameter gateway")
});

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
    if crate::utils::misc::env::is_env_truthy("FOREST_TEST_SKIP_PROOF_PARAM_CHECK") {
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
) -> anyhow::Result<()> {
    // Just print out the parameters download directory path and exit.
    if dry_run {
        println!("{}", param_dir(data_dir).to_string_lossy());
        return Ok(());
    }

    fs::create_dir_all(param_dir(data_dir)).await?;

    let params: ParameterMap = serde_json::from_str(param_json)?;
    let sources = &param_sources()?;

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
                fetch_verify_params(data_dir, &name, Arc::new(info), sources).await
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
) -> anyhow::Result<()> {
    get_params(data_dir, DEFAULT_PARAMETERS, storage_size, dry_run).await
}

async fn fetch_verify_params(
    data_dir: &Path,
    name: &str,
    info: Arc<ParameterData>,
    sources: &[ParamSource],
) -> anyhow::Result<()> {
    let path: PathBuf = param_dir(data_dir).join(name);

    match check_parameter_file(&path, &info).await {
        Ok(()) => return Ok(()),
        // A missing file is the normal case, it is downloaded below.
        Err(e)
            if e.downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == ErrorKind::NotFound) => {}
        Err(e) => warn!("Error checking file: {e:?}"),
    }

    let mut last_error = None;
    for source in sources {
        info!(
            "Fetching param file {path} from {source}",
            path = path.display()
        );
        let fetched = async {
            source.download(&info.cid, &path).await?;
            check_parameter_file(&path, &info).await
        }
        .await;
        match fetched {
            Ok(()) => return Ok(()),
            Err(e) => {
                warn!("Failed to fetch param file from {source}: {e:?}");
                last_error = Some(e);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("no proof parameter source configured")))
}

/// A source of proof parameter files, addressed by CID.
#[derive(derive_more::Display)]
#[display("{_0}")]
enum ParamSource {
    /// Raw file, verified by [`check_parameter_file`] once downloaded.
    Mirror(Url),
    /// Trustless IPFS gateway, which verifies the CID while decoding the CAR response.
    IpfsGateway(Url),
}

impl ParamSource {
    async fn download(&self, cid: &Cid, path: &Path) -> anyhow::Result<()> {
        match self {
            Self::Mirror(base) => {
                let url = base.join(&cid.to_string())?;
                download_to(&url, path, DownloadFileOption::NonResumable, None)
                    .await
                    .with_context(|| format!("failed to download {url}"))
            }
            Self::IpfsGateway(gateway) => {
                (|| download_ipfs_file_trustlessly(cid, gateway, path))
                    .retry(ExponentialBuilder::default())
                    .notify(|err, dur| {
                        debug!(
                            "retrying download_ipfs_file_trustlessly {err} after {}",
                            humantime::format_duration(dur)
                        );
                    })
                    .await
            }
        }
    }
}

/// [`Url::join`] replaces the last path segment unless the path ends with a separator.
fn with_trailing_slash(mut url: Url) -> Url {
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    url
}

fn param_sources() -> anyhow::Result<Vec<ParamSource>> {
    crate::def_is_env_truthy!(force_ipfs_gateway, PROOFS_ONLY_IPFS_GATEWAY_ENV);

    let gateway = ParamSource::IpfsGateway(match std::env::var(IPFS_GATEWAY_ENV) {
        Ok(gateway) => with_trailing_slash(
            Url::parse(&gateway).with_context(|| format!("invalid {IPFS_GATEWAY_ENV}"))?,
        ),
        Err(_) => DEFAULT_IPFS_GATEWAY.clone(),
    });

    Ok(if force_ipfs_gateway() {
        vec![gateway]
    } else {
        vec![
            ParamSource::Mirror(CHAINSAFE_PROOF_PARAMETER_GATEWAY.clone()),
            gateway,
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Itertools as _;

    #[test]
    #[serial_test::serial]
    fn mirror_is_tried_before_the_gateway() {
        unsafe {
            std::env::remove_var(PROOFS_ONLY_IPFS_GATEWAY_ENV);
            std::env::remove_var(IPFS_GATEWAY_ENV);
        }
        let sources = param_sources()
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect_vec();
        assert_eq!(
            sources,
            [
                "https://filecoin-proofs.chainsafe.dev/ipfs/",
                "https://proofs.filecoin.io/ipfs/"
            ]
        );
    }

    #[test]
    #[serial_test::serial]
    fn forced_gateway_drops_the_mirror() {
        unsafe {
            std::env::set_var(PROOFS_ONLY_IPFS_GATEWAY_ENV, "1");
            std::env::set_var(IPFS_GATEWAY_ENV, "https://example.com/ipfs");
        }
        let sources = param_sources()
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect_vec();
        assert_eq!(sources, ["https://example.com/ipfs/"]);
        unsafe {
            std::env::remove_var(PROOFS_ONLY_IPFS_GATEWAY_ENV);
            std::env::remove_var(IPFS_GATEWAY_ENV);
        }
    }
}
