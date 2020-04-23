// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_libp2p::Libp2pConfig;
use serde::Deserialize;
use utils::get_home_dir;

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub network: Libp2pConfig,
    pub data_dir: String,
    pub genesis_file: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            network: Libp2pConfig::default(),
            data_dir: get_home_dir() + "/.forest",
            genesis_file: None,
        }
    }
}
