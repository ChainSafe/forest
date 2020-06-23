use async_std::fs;
use blake2b_simd::blake2b;
use fil_types::SectorSize;
use log::{info, warn};
use pbr::{ProgressBar, Units};
use reqwest::{blocking::Client, header, Proxy, Url};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error as StdError;
use std::fs::File;
use std::io::{self, copy, prelude::*, BufReader, ErrorKind, Stdout};
use std::path::{Path, PathBuf};

const GATEWAY: &str = "https://proofs.filecoin.io/ipfs/";
const PARAM_DIR: &str = "/var/tmp/filecoin-proof-parameters";
const DIR_ENV: &str = "FIL_PROOFS_PARAMETER_CACHE";
const GATEWAY_ENV: &str = "IPFS_GATEWAY";
const TRUST_PARAMS_ENV: &str = "TRUST_PARAMS";

const DEFAULT_PARAMETERS: &str = include_str!("parameters.json");

type ParameterMap = HashMap<String, ParameterData>;

struct FetchProgress<R> {
    inner: R,
    progress_bar: ProgressBar<Stdout>,
}

impl<R: Read> Read for FetchProgress<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf).map(|n| {
            self.progress_bar.add(n as u64);
            n
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ParameterData {
    pub cid: String,
    pub digest: String,
    pub sector_size: u64,
}

#[inline]
fn param_dir() -> String {
    std::env::var(DIR_ENV).unwrap_or(PARAM_DIR.to_owned())
}

/// Get proofs parameters and all verification keys for a given sector size given
/// a param JSON manifest.
pub async fn get_params(
    param_json: &str,
    storage_size: SectorSize,
) -> Result<(), Box<dyn StdError>> {
    fs::create_dir_all(param_dir()).await?;

    let params: ParameterMap = serde_json::from_str(param_json)?;

    // TODO make async lol
    for (name, info) in params {
        if storage_size as u64 != info.sector_size && name.ends_with(".params") {
            continue;
        }

        if let Err(e) = fetch_verify_params(&name, &info).await {
            warn!("Error in validating params {}", e);
        }
    }

    Ok(())
}

/// Get proofs parameters and all verification keys for a given sector size using default manifest.
#[inline]
pub async fn get_params_default(storage_size: SectorSize) -> Result<(), Box<dyn StdError>> {
    get_params(DEFAULT_PARAMETERS, storage_size).await
}

async fn fetch_verify_params(name: &str, info: &ParameterData) -> Result<(), Box<dyn StdError>> {
    let mut path: PathBuf = param_dir().into();
    path.push(name);

    match check_file(&path, info) {
        Ok(()) => return Ok(()),
        Err(e) => warn!("{}", e),
    }

    fetch_params(&path, info).await?;

    check_file(&path, info).map_err(|e| {
        // TODO remove invalid file
        e.into()
    })
}

async fn fetch_params(path: &Path, info: &ParameterData) -> Result<(), Box<dyn StdError>> {
    let gw = std::env::var(GATEWAY_ENV).unwrap_or(GATEWAY.to_owned());
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
            Err(format!("failed to download file: {}", url))?
        }
    };

    let req = client.get(url.as_str());

    let mut pb = ProgressBar::new(total_size);
    pb.set_units(Units::Bytes);

    let mut source = FetchProgress {
        inner: req.send()?,
        progress_bar: pb,
    };

    let _ = copy(&mut source, &mut file)?;

    Ok(())
}

fn check_file(path: &Path, info: &ParameterData) -> Result<(), io::Error> {
    if std::env::var(TRUST_PARAMS_ENV) == Ok("1".to_owned()) {
        warn!("Assuming parameter files are okay. DO NOT USE IN PRODUCTION");
        return Ok(());
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let sum = blake2b(reader.buffer());
    let str_sum = sum.to_hex();
    if &str_sum[..16] == info.digest {
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
