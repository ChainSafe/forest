// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use chain_sync::SyncConfig;
use forest_libp2p::Libp2pConfig;
use networks::ChainConfig;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::miscellaneous::Miscellaneous;

#[derive(Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Config {
    pub miscellaneous: Miscellaneous,
    pub rocks_db: forest_db::rocks_config::RocksDbConfig,
    pub network: Libp2pConfig,
    pub sync: SyncConfig,
    pub chain: Arc<ChainConfig>,
}
