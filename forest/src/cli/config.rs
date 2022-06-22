// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use chain_sync::SyncConfig;
use forest_libp2p::Libp2pConfig;
use networks::ChainConfig;
use rpc_client::DEFAULT_PORT;
use serde::{Deserialize, Serialize};
use utils::get_home_dir;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub data_dir: String,
    pub genesis_file: Option<String>,
    pub enable_rpc: bool,
    pub rpc_port: u16,
    pub rpc_token: Option<String>,
    /// If this is true, then we do not validate the imported snapshot.
    /// Otherwise, we validate and compute the states.
    pub snapshot: bool,
    pub snapshot_height: Option<i64>,
    pub snapshot_path: Option<String>,
    /// Skips loading import CAR file and assumes it's already been loaded.
    /// Will use the cids in the header of the file to index the chain.
    pub skip_load: bool,
    pub encrypt_keystore: bool,
    pub metrics_port: u16,
    pub rocks_db: db::rocks_config::RocksDbConfig,
    pub network: Libp2pConfig,
    pub sync: SyncConfig,
    pub chain: ChainConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            network: Libp2pConfig::default(),
            data_dir: get_home_dir() + "/.forest",
            genesis_file: None,
            enable_rpc: true,
            rpc_port: DEFAULT_PORT,
            rpc_token: None,
            snapshot_path: None,
            snapshot: false,
            snapshot_height: None,
            skip_load: false,
            sync: SyncConfig::default(),
            encrypt_keystore: true,
            metrics_port: 6116,
            rocks_db: db::rocks_config::RocksDbConfig::default(),
            chain: ChainConfig::default(),
        }
    }
}
