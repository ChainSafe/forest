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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            network: Libp2pConfig::default(),
            data_dir: get_home_dir() + "/.forest",
            genesis_file: None,
            drand_public: DrandPublic{coefficient: hex::decode("868f005eb8e6e4ca0a47c8a77ceaa5309a47978a7c71bc5cce96366b5d7a569937c529eeda66c7293784a9402801af31").unwrap()},
            enable_rpc : true,
            rpc_port: "1234".to_string(),
        }
    }
}
