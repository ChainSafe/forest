// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use beacon::DrandPublic;
use forest_libp2p::Libp2pConfig;
use serde::Deserialize;
use utils::get_home_dir;
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub network: Libp2pConfig,
    pub data_dir: String,
    pub genesis_file: Option<String>,
    pub drand_public: DrandPublic,
    pub enable_rpc: bool,
    pub rpc_port: String,
    /// If this is true, then we do not validate the imported snapshot.
    /// Otherwise, we validate and compute the states.
    pub snapshot: bool,
    pub snapshot_path: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            network: Libp2pConfig::default(),
            data_dir: get_home_dir() + "/.forest",
            genesis_file: None,
            drand_public: DrandPublic{coefficient: hex::decode("8cad0c72c606ab27d36ee06de1d5b2db1faf92e447025ca37575ab3a8aac2eaae83192f846fc9e158bc738423753d000").unwrap()},
            enable_rpc : true,
            rpc_port: "1234".to_string(),
            snapshot_path: None,
            snapshot: false,
        }
    }
}
