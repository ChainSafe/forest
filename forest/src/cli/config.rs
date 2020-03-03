// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_libp2p::Libp2pConfig;
use serde::Deserialize;
use utils::get_home_dir;

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub network: Libp2pConfig,
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
}

fn default_data_dir() -> String {
    get_home_dir() + "/.forest"
}
