// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::{
    channel::bounded,
    fs::{self, File},
    io::{copy, BufWriter},
    sync::Arc,
    task,
};
use blake2b_simd::{Hash, State as Blake2b};
use core::time::Duration;
use fil_types::SectorSize;
use log::{info, warn};
use net_utils::FetchProgress;
use pbr::{MultiBar, Units};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error as StdError;
use std::fs::File as SyncFile;
use std::io::{self, copy as sync_copy, BufReader as SyncBufReader, ErrorKind, Stdout};
use std::path::{Path, PathBuf};
use surf::Client;

const GATEWAY: &str = "https://proofs.filecoin.io/ipfs/";
const PARAM_DIR: &str = "/var/tmp/filecoin-proof-parameters";
const DIR_ENV: &str = "FIL_PROOFS_PARAMETER_CACHE";
const GATEWAY_ENV: &str = "IPFS_GATEWAY";
const TRUST_PARAMS_ENV: &str = "TRUST_PARAMS";
const DEFAULT_PARAMETERS: &str = include_str!("parameters.json");

/// Sector size options for fetching.
pub enum SectorSizeOpt {
    /// All keys and proofs gen params
    All,
    /// Only verification params
    Keys,
    /// All keys and proofs gen params for a given size
    Size(SectorSize),
}

type ParameterMap = HashMap<String, ParameterData>;

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ParameterData {
    cid: String,
    digest: String,
    sector_size: u64,
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

    params
        .into_iter()
        .filter(|(name, info)| match storage_size {
            SectorSizeOpt::Keys => !name.ends_with("params"),
            SectorSizeOpt::Size(size) => {
                size as u64 == info.sector_size || !name.ends_with(".params")
            }
            SectorSizeOpt::All => true,
        })
        .for_each(|(name, info)| {
            let cmb = mb.clone();
            tasks.push(task::spawn(async move {
                if let Err(e) = fetch_verify_params(&name, Arc::new(info), cmb).await {
                    warn!("Error in validating params {}", e);
                }
            }))
        });

    if let Some(multi_bar) = mb {
        let cmb = multi_bar.clone();
        let (mb_send, mb_rx) = bounded(1);
        let mb = task::spawn(async move {
            while mb_rx.try_recv().is_err() {
                cmb.listen();
                task::sleep(Duration::from_millis(1000)).await;
            }
        });
        for t in tasks {
            t.await;
        }
        mb_send
            .send(())
            .await
            .expect("Receiver should not be dropped");
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
    info: Arc<ParameterData>,
    mb: Option<Arc<MultiBar<Stdout>>>,
) -> Result<(), Box<dyn StdError>> {
    let mut path: PathBuf = param_dir().into();
    path.push(name);
    let path: Arc<Path> = Arc::from(path.as_path());

    match check_file(path.clone(), info.clone()).await {
        Ok(()) => return Ok(()),
        Err(e) => {
            if e.kind() != ErrorKind::NotFound {
                warn!("Error checking file: {}", e);
            }
        }
    }

    fetch_params(&path, &info, mb).await?;

    check_file(path, info).await.map_err(|e| {
        // TODO remove invalid file
        e.into()
    })
}

async fn fetch_params(
    path: &Path,
    info: &ParameterData,
    multi_bar: Option<Arc<MultiBar<Stdout>>>,
) -> Result<(), Box<dyn StdError>> {
    let gw = std::env::var(GATEWAY_ENV).unwrap_or_else(|_| GATEWAY.to_owned());
    info!("Fetching {:?} from {}", path, gw);

    let file = File::create(path).await?;
    let mut writer = BufWriter::new(file);

    let url = format!("{}{}", gw, info.cid);

    let client = Client::new();

    let req = client.get(url.as_str());

    if let Some(mb) = multi_bar {
        let total_size: u64 = {
            // TODO head requests are broken in Surf right now, revisit when fixed
            // let res = client.head(&url).await?;
            // if res.status().is_success() {
            //     res.header("Content-Length")
            //         .and_then(|len| len.as_str().parse().ok())
            //         .unwrap_or_default()
            // } else {
            //     return Err(format!("failed to download file: {}", url).into());
            // }
            100
        };
        let mut pb = mb.create_bar(total_size);
        pb.set_units(Units::Bytes);

        let mut source = FetchProgress {
            inner: req.await?,
            progress_bar: pb,
        };
        copy(&mut source, &mut writer).await?;
        source.finish();
    } else {
        let mut source = req.await?;
        copy(&mut source, &mut writer).await?;
    };

    Ok(())
}

async fn check_file(path: Arc<Path>, info: Arc<ParameterData>) -> Result<(), io::Error> {
    if std::env::var(TRUST_PARAMS_ENV) == Ok("1".to_owned()) {
        warn!("Assuming parameter files are okay. DO NOT USE IN PRODUCTION");
        return Ok(());
    }

    let cloned_path = path.clone();
    let hash = task::spawn_blocking(move || -> Result<Hash, io::Error> {
        let file = SyncFile::open(cloned_path.as_ref())?;
        let mut reader = SyncBufReader::new(file);
        let mut hasher = Blake2b::new();
        sync_copy(&mut reader, &mut hasher)?;
        Ok(hasher.finalize())
    })
    .await?;

    let str_sum = hash.to_hex();
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
