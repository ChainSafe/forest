// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::{fs, sync::Arc, task};
use blake2b_simd::State as Blake2b;
use core::time::Duration;
use fil_types::SectorSize;
use log::{info, warn};
use pbr::{MultiBar, ProgressBar, Units};
use reqwest::{blocking::Client, header, Proxy, Url};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error as StdError;
use std::fs::File;
use std::io::{self, copy, prelude::*, ErrorKind, Stdout};
use std::path::{Path, PathBuf};

const GATEWAY: &str = "https://proofs.filecoin.io/ipfs/";
const PARAM_DIR: &str = "/var/tmp/filecoin-proof-parameters";
const DIR_ENV: &str = "FIL_PROOFS_PARAMETER_CACHE";
const GATEWAY_ENV: &str = "IPFS_GATEWAY";
const TRUST_PARAMS_ENV: &str = "TRUST_PARAMS";
const DEFAULT_PARAMETERS: &str = include_str!("parameters.json");

/// Sector size options for fetching
pub enum SectorSizeOpt {
    /// All keys and proofs gen params
    All,
    /// Only verification params
    Keys,
    /// All keys and proofs gen params for a given size
    Size(SectorSize),
}

type ParameterMap = HashMap<String, ParameterData>;

#[derive(Debug, Deserialize, Serialize)]
pub struct ParameterData {
    pub cid: String,
    pub digest: String,
    pub sector_size: u64,
}

#[inline]
fn param_dir() -> String {
    std::env::var(DIR_ENV).unwrap_or_else(|_| PARAM_DIR.to_owned())
}

/// Get proofs parameters and all verification keys for a given sector size given
/// a param JSON manifest.
pub async fn get_params(
    param_json: &str,
    storage_size: SectorSizeOpt,
    is_verbose: bool,
) -> Result<(), Box<dyn StdError>> {
    fs::create_dir_all(param_dir()).await?;

    let params: ParameterMap = serde_json::from_str(param_json)?;
    let mut tasks = Vec::with_capacity(params.len());

    let mb = if is_verbose {
        Some(Arc::new(MultiBar::new()))
    } else {
        None
    };

    for (name, info) in params {
        match storage_size {
            SectorSizeOpt::All => (),
            SectorSizeOpt::Keys => {
                if name.ends_with(".params") {
                    continue;
                }
            }
            SectorSizeOpt::Size(size) => {
                if size as u64 != info.sector_size && name.ends_with(".params") {
                    continue;
                }
            }
        }

        let cmb = mb.clone();
        tasks.push(task::spawn(async move {
            if let Err(e) = fetch_verify_params(&name, &info, cmb).await {
                warn!("Error in validating params {}", e);
            }
        }));
    }

    if let Some(multi_bar) = mb {
        let cmb = multi_bar.clone();
        let (mb_send, mut mb_rx) = futures::channel::oneshot::channel();
        let mb = task::spawn(async move {
            while mb_rx.try_recv() == Ok(None) {
                cmb.listen();
                task::sleep(Duration::from_millis(1000)).await;
            }
        });
        for t in tasks {
            t.await;
        }
        mb_send.send(()).unwrap();
        mb.await;
    } else {
        for t in tasks {
            t.await;
        }
    }

    Ok(())
}

/// Get proofs parameters and all verification keys for a given sector size using default manifest.
#[inline]
pub async fn get_params_default(
    storage_size: SectorSizeOpt,
    is_verbose: bool,
) -> Result<(), Box<dyn StdError>> {
    get_params(DEFAULT_PARAMETERS, storage_size, is_verbose).await
}

async fn fetch_verify_params(
    name: &str,
    info: &ParameterData,
    mb: Option<Arc<MultiBar<Stdout>>>,
) -> Result<(), Box<dyn StdError>> {
    let mut path: PathBuf = param_dir().into();
    path.push(name);

    match check_file(&path, info) {
        Ok(()) => return Ok(()),
        Err(e) => {
            if e.kind() != ErrorKind::NotFound {
                warn!("{}", e)
            }
        }
    }

    fetch_params(&path, info, mb).await?;

    check_file(&path, info).map_err(|e| {
        // TODO remove invalid file
        e.into()
    })
}

struct FetchProgress<R> {
    inner: R,
    progress_bar: ProgressBar<pbr::Pipe>,
}

impl<R> FetchProgress<R> {
    fn finish(&mut self) {
        self.progress_bar.finish();
    }
}

impl<R: Read> Read for FetchProgress<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf).map(|n| {
            self.progress_bar.add(n as u64);
            n
        })
    }
}

async fn fetch_params(
    path: &Path,
    info: &ParameterData,
    multi_bar: Option<Arc<MultiBar<Stdout>>>,
) -> Result<(), Box<dyn StdError>> {
    let gw = std::env::var(GATEWAY_ENV).unwrap_or_else(|_| GATEWAY.to_owned());
    info!("Fetching {:?} from {}", path, gw);

    let mut file = File::create(path)?;

    let url = Url::parse(&format!("{}{}", gw, info.cid))?;

    let client = Client::builder()
        .proxy(Proxy::custom(move |url| env_proxy::for_url(&url).to_url()))
        .build()?;
    let total_size = {
        let res = client.head(url.as_str()).send()?;
        if res.status().is_success() {
            res.headers()
                .get(header::CONTENT_LENGTH)
                .and_then(|ct_len| ct_len.to_str().ok())
                .and_then(|ct_len| ct_len.parse().ok())
                .unwrap_or(0)
        } else {
            return Err(format!("failed to download file: {}", url).into());
        }
    };

    let req = client.get(url.as_str());

    if let Some(mb) = multi_bar {
        let mut pb = mb.create_bar(total_size);
        pb.set_units(Units::Bytes);

        let mut source = FetchProgress {
            inner: req.send()?,
            progress_bar: pb,
        };
        copy(&mut source, &mut file)?;
        source.finish();
    } else {
        let mut source = req.send()?;
        copy(&mut source, &mut file)?;
    };

    Ok(())
}

fn check_file(path: &Path, info: &ParameterData) -> Result<(), io::Error> {
    if std::env::var(TRUST_PARAMS_ENV) == Ok("1".to_owned()) {
        warn!("Assuming parameter files are okay. DO NOT USE IN PRODUCTION");
        return Ok(());
    }

    let mut file = File::open(path)?;
    let mut hasher = Blake2b::new();
    copy(&mut file, &mut hasher)?;

    let str_sum = hasher.finalize().to_hex();
    let str_sum = &str_sum[..32];
    if str_sum == info.digest {
        info!("Parameter file {:?} is ok", path);
        Ok(())
    } else {
        Err(io::Error::new(
            ErrorKind::Other,
            format!(
                "Checksum mismatch in param file {:?}. ({} != {})",
                path, str_sum, info.digest
            ),
        ))
    }
}
