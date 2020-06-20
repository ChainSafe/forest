use fil_types::SectorSize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{fs, io};

const GATEWAY: &str = "https://proofs.filecoin.io/ipfs/";
const PARAM_DIR: &str = "/var/tmp/filecoin-proof-parameters";
const DIR_ENV: &str = "FIL_PROOFS_PARAMETER_CACHE";

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

/// Get proofs parameters for a given sector size given a param JSON manifest.
pub fn get_params(param_json: &str, storage_size: SectorSize) -> Result<(), io::Error> {
    fs::create_dir_all(param_dir())?;

    let params: ParameterMap = serde_json::from_str(param_json)?;

    // TODO make async lol
    for (name, info) in params {
        if storage_size as u64 != info.sector_size && name.ends_with(".params") {
            continue;
        }

        fetch_params(&name, &info);
    }

    Ok(())
}

/// Get proofs parameters for a given sector size using default manifest.
pub fn get_params_default(storage_size: SectorSize) -> Result<(), io::Error> {
    get_params(DEFAULT_PARAMETERS, storage_size)
}

pub fn fetch_params(_name: &str, _info: &ParameterData) {
    todo!()
}
