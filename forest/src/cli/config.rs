// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use chain_sync::SyncConfig;
use forest_libp2p::Libp2pConfig;
use serde::Deserialize;
use utils::get_home_dir;

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub network: Libp2pConfig,
    pub data_dir: String,
    pub genesis_file: Option<String>,
    pub enable_rpc: bool,
    pub rpc_port: String,
    /// If this is true, then we do not validate the imported snapshot.
    /// Otherwise, we validate and compute the states.
    pub snapshot: bool,
    pub snapshot_path: Option<String>,
    /// Skips loading import CAR file and assumes it's already been loaded.
    /// Will use the cids in the header of the file to index the chain.
    pub skip_load: bool,
    pub sync: SyncConfig,
    pub encrypt_keystore: bool,
    pub metrics_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            network: Libp2pConfig::default(),
            data_dir: get_home_dir() + "/.forest",
            genesis_file: None,
            enable_rpc: true,
            rpc_port: "1234".to_string(),
            snapshot_path: None,
            snapshot: false,
            skip_load: false,
            sync: SyncConfig::default(),
            encrypt_keystore: false,
            metrics_port: 6116,
        }
    }
}
