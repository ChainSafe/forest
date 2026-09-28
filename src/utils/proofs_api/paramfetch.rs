// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! This module contains the logic for fetching the proofs parameters from the network.
//! Every source is addressed by the parameter file CID, so the file name is only used for the local cache.
//!
//! Every download is checked against the digest from the parameter manifest, unless
//! `FOREST_FORCE_TRUST_PARAMS` says otherwise, so the mirrors do not have to be trusted.

use std::{
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    sync::LazyLock,
};

use crate::{
    shim::sector::SectorSize,
    utils::net::{DownloadFileOption, download_to},
};
use anyhow::Context as _;
use futures::{TryStreamExt, stream::FuturesUnordered};
use tokio::{fs, sync::Mutex};
use tracing::{info, warn};
use url::Url;

#[cfg(test)]
use super::parameters::PROOF_DIGEST_LEN;
use super::parameters::{
    DEFAULT_PARAMETERS, PROOFS_PARAMETER_CACHE_ENV, ParameterData, ParameterMap,
    check_parameter_file, param_dir,
};

static CHAINSAFE_PROOF_PARAMETER_MIRROR: LazyLock<Url> = LazyLock::new(|| {
    Url::parse("https://filecoin-proofs.chainsafe.dev/ipfs/")
        .expect("invalid ChainSafe proof parameter mirror")
});

/// Independently operated mirror, so that [`CHAINSAFE_PROOF_PARAMETER_MIRROR`] is not a single point of failure.
/// <https://github.com/filecoin-project/lotus/issues/12273#issuecomment-5718053900>
static FALLBACK_PROOF_PARAMETER_MIRROR: LazyLock<Url> = LazyLock::new(|| {
    Url::parse("https://vault.ezpdpz.net/ipfs/").expect("invalid fallback proof parameter mirror")
});

/// Mirror to fetch parameters from, replacing the default ones.
///
/// The defaults are hosted outside of China and are very slow to download from there, so users in such regions
/// should point this at a mirror close to them.
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
                fetch_verify_params(data_dir, &name, &info, sources).await
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
    info: &ParameterData,
    sources: &[Url],
) -> anyhow::Result<()> {
    let path: PathBuf = param_dir(data_dir).join(name);

    match check_parameter_file(&path, info).await {
        Ok(()) => return Ok(()),
        // A missing file is the normal case, it is downloaded below.
        Err(e)
            if e.downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == ErrorKind::NotFound) => {}
        Err(e) => warn!("Error checking file: {e:?}"),
    }

    let cid = info.cid.to_string();
    let mut last_error = None;
    for source in sources {
        let url = source.join(&cid)?;
        info!(
            "Fetching param file {path} from {url}",
            path = path.display()
        );
        let fetched = async {
            download_to(&url, &path, DownloadFileOption::NonResumable, None)
                .await
                .with_context(|| format!("failed to download {url}"))?;
            check_parameter_file(&path, info).await
        }
        .await;
        match fetched {
            Ok(()) => return Ok(()),
            Err(e) => {
                warn!("Failed to fetch param file from {url}: {e:?}");
                last_error = Some(e);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("no proof parameter source configured")))
}

/// [`Url::join`] replaces the last path segment unless the path ends with a separator.
fn with_trailing_slash(mut url: Url) -> Url {
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    url
}

fn param_sources() -> anyhow::Result<Vec<Url>> {
    let custom = match std::env::var(IPFS_GATEWAY_ENV) {
        Ok(gateway) => Some(with_trailing_slash(
            Url::parse(&gateway).with_context(|| format!("invalid {IPFS_GATEWAY_ENV}"))?,
        )),
        Err(_) => None,
    };

    Ok(param_sources_from(custom))
}

fn param_sources_from(custom: Option<Url>) -> Vec<Url> {
    match custom {
        Some(custom) => vec![custom],
        None => vec![
            CHAINSAFE_PROOF_PARAMETER_MIRROR.clone(),
            FALLBACK_PROOF_PARAMETER_MIRROR.clone(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cid::Cid;
    use itertools::Itertools as _;
    use rstest::rstest;

    const CHAINSAFE: &str = "https://filecoin-proofs.chainsafe.dev/ipfs/";
    const FALLBACK: &str = "https://vault.ezpdpz.net/ipfs/";

    #[rstest]
    #[case(None, &[CHAINSAFE, FALLBACK])]
    #[case(Some("https://example.com/ipfs"), &["https://example.com/ipfs/"])]
    fn a_custom_mirror_replaces_the_defaults(
        #[case] custom: Option<&str>,
        #[case] expected: &[&str],
    ) {
        let custom = custom.map(|custom| with_trailing_slash(custom.parse().unwrap()));
        let sources = param_sources_from(custom)
            .iter()
            .map(ToString::to_string)
            .collect_vec();
        assert_eq!(sources, expected);
    }

    /// Serves `content` on any path, or 500 when it is `None`.
    async fn serve(content: Option<&'static [u8]>) -> Url {
        let app = axum::Router::new().fallback(move || async move {
            match content {
                Some(content) => (axum::http::StatusCode::OK, content),
                None => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, &b""[..]),
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}/").parse().unwrap()
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn sources_are_tried_until_one_serves_the_expected_digest() {
        const CONTENT: &[u8] = b"Cthulhu fhtagn!";

        let data_dir = tempfile::TempDir::new().unwrap();
        unsafe { std::env::set_var(PROOFS_PARAMETER_CACHE_ENV, data_dir.path()) };
        fs::create_dir_all(param_dir(data_dir.path()))
            .await
            .unwrap();

        let info = ParameterData {
            cid: Cid::default(),
            digest: blake2b_simd::blake2b(CONTENT).as_bytes()[..PROOF_DIGEST_LEN]
                .try_into()
                .unwrap(),
            sector_size: 2048,
        };

        let sources = [
            serve(None).await,
            serve(Some(b"not the expected content")).await,
            serve(Some(CONTENT)).await,
        ];
        fetch_verify_params(data_dir.path(), "test.vk", &info, &sources)
            .await
            .unwrap();

        let downloaded = fs::read(data_dir.path().join("test.vk")).await.unwrap();
        assert_eq!(downloaded, CONTENT);
        unsafe { std::env::remove_var(PROOFS_PARAMETER_CACHE_ENV) };
    }
}
