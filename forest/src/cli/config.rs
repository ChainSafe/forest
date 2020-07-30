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
            drand_public: DrandPublic{coefficient: hex::decode("922a2e93828ff83345bae533f5172669a26c02dc76d6bf59c80892e12ab1455c229211886f35bb56af6d5bea981024df").unwrap()},
            enable_rpc : true,
            rpc_port: "1234".to_string()
        }
    }
}
