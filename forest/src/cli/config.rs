// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use chain_sync::SyncConfig;
use forest_libp2p::Libp2pConfig;
use networks::ChainConfig;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use super::miscellaneous::Miscellaneous;

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub miscellaneous: Miscellaneous,
    /// Metrics bind, e.g. 127.0.0.1:6116
    pub metrics_address: SocketAddr,
    pub rocks_db: forest_db::rocks_config::RocksDbConfig,
    pub network: Libp2pConfig,
    pub sync: SyncConfig,
    pub chain: Arc<ChainConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            miscellaneous: Miscellaneous::default(),
            network: Libp2pConfig::default(),
            sync: SyncConfig::default(),
            metrics_address: FromStr::from_str("127.0.0.1:6116").unwrap(),
            rocks_db: forest_db::rocks_config::RocksDbConfig::default(),
            chain: Arc::default(),
        }
    }
}
