use async_std::fs;
use blake2b_simd::blake2b;
use fil_types::SectorSize;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, ErrorKind};
use std::path::{Path, PathBuf};

const GATEWAY: &str = "https://proofs.filecoin.io/ipfs/";
const PARAM_DIR: &str = "/var/tmp/filecoin-proof-parameters";
const DIR_ENV: &str = "FIL_PROOFS_PARAMETER_CACHE";
const GATEWAY_ENV: &str = "IPFS_GATEWAY";
const TRUST_PARAMS: &str = "TRUST_PARAMS";

const DEFAULT_PARAMETERS: &str = include_str!("parameters.json");

type ParameterMap = HashMap<String, ParameterData>;

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
pub async fn get_params(param_json: &str, storage_size: SectorSize) -> Result<(), io::Error> {
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
pub async fn get_params_default(storage_size: SectorSize) -> Result<(), io::Error> {
    get_params(DEFAULT_PARAMETERS, storage_size).await
}

async fn fetch_verify_params(name: &str, info: &ParameterData) -> Result<(), io::Error> {
    let mut path: PathBuf = param_dir().into();
    path.push(name);

    match check_file(&path, info) {
        Ok(()) => return Ok(()),
        Err(e) => warn!("{}", e),
    }

    fetch_params(&path, info).await?;

    check_file(&path, info).map_err(|e| {
        // TODO remove invalid file
        e
    })
}

async fn fetch_params(_path: &Path, _info: &ParameterData) -> Result<(), io::Error> {
    todo!()
}

fn check_file(path: &Path, info: &ParameterData) -> Result<(), io::Error> {
    if std::env::var(TRUST_PARAMS) == Ok("1".to_owned()) {
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
