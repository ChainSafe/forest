// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use beacon::DistPublic;
use forest_libp2p::Libp2pConfig;
use serde::Deserialize;
use utils::get_home_dir;
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub network: Libp2pConfig,
    pub data_dir: String,
    pub genesis_file: Option<String>,
    pub drand_dist_public: DistPublic,
    pub enable_rpc: bool,
    pub rpc_port: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            network: Libp2pConfig::default(),
            data_dir: get_home_dir() + "/.forest",
            genesis_file: None,
            drand_dist_public: DistPublic{coefficients: [hex::decode("82c279cce744450e68de98ee08f9698a01dd38f8e3be3c53f2b840fb9d09ad62a0b6b87981e179e1b14bc9a2d284c985").unwrap(),
                hex::decode("82d51308ad346c686f81b8094551597d7b963295cbf313401a93df9baf52d5ae98a87745bee70839a4d6e65c342bd15b").unwrap(),
                hex::decode("94eebfd53f4ba6a3b8304236400a12e73885e5a781509a5c8d41d2e8b476923d8ea6052649b3c17282f596217f96c5de").unwrap(),
                hex::decode("8dc4231e42b4edf39e86ef1579401692480647918275da767d3e558c520d6375ad953530610fd27daf110187877a65d0").unwrap(),]},
            enable_rpc : true,
            rpc_port: "1234".to_string()
        }
    }
}
