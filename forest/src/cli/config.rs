// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use chain_sync::SyncConfig;
use directories::ProjectDirs;
use forest_libp2p::Libp2pConfig;
use networks::ChainConfig;
use rpc_client::DEFAULT_PORT;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub data_dir: PathBuf,
    pub genesis_file: Option<String>,
    pub enable_rpc: bool,
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
    /// Metrics bind, e.g. 127.0.0.1:6116
    pub metrics_address: SocketAddr,
    /// RPC bind, e.g. 127.0.0.1:1234
    pub rpc_address: SocketAddr,
    pub rocks_db: forest_db::rocks_config::RocksDbConfig,
    pub network: Libp2pConfig,
    pub sync: SyncConfig,
    pub chain: Arc<ChainConfig>,
}

impl Default for Config {
    fn default() -> Self {
        let dir = ProjectDirs::from("com", "ChainSafe", "Forest").expect("failed to find project directories, please set FOREST_CONFIG_PATH environment variable manually.");
        Self {
            network: Libp2pConfig::default(),
            data_dir: dir.data_dir().to_path_buf(),
            genesis_file: None,
            enable_rpc: true,
            rpc_token: None,
            snapshot_path: None,
            snapshot: false,
            snapshot_height: None,
            skip_load: false,
            sync: SyncConfig::default(),
            encrypt_keystore: true,
            metrics_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 6116),
            rpc_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_PORT),
            rocks_db: forest_db::rocks_config::RocksDbConfig::default(),
            chain: Arc::default(),
        }
    }
}
