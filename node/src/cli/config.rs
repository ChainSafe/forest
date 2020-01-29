// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use forest_libp2p::config::Libp2pConfig;
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub network: Libp2pConfig,
}
