// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    str::FromStr,
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use crate::daemon::db_util::ImportMode;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct ChunkSize(pub u32);
impl Default for ChunkSize {
    fn default() -> Self {
        ChunkSize(500_000)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct BufferSize(pub u32);
impl Default for BufferSize {
    fn default() -> Self {
        BufferSize(1)
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct Client {
    pub data_dir: PathBuf,
    pub genesis_file: Option<PathBuf>,
    pub enable_rpc: bool,
    pub enable_metrics_endpoint: bool,
    pub enable_health_check: bool,
    pub snapshot_height: Option<i64>,
    pub snapshot_head: Option<i64>,
    pub snapshot_path: Option<PathBuf>,
    pub import_mode: ImportMode,
    /// Skips loading import CAR file and assumes it's already been loaded.
    /// Will use the CIDs in the header of the file to index the chain.
    pub skip_load: bool,
    /// When importing CAR files, chunk key-value pairs before committing them
    /// to the database.
    pub chunk_size: ChunkSize,
    /// When importing CAR files, maintain a read-ahead buffer measured in
    /// number of chunks.
    pub buffer_size: BufferSize,
    pub encrypt_keystore: bool,
    /// Metrics bind, e.g. 127.0.0.1:6116
    pub metrics_address: SocketAddr,
    /// RPC bind, e.g. 127.0.0.1:1234
    pub rpc_address: SocketAddr,
    /// Path to a list of RPC methods to allow/disallow.
    pub rpc_filter_list: Option<PathBuf>,
    /// Healthcheck bind, e.g. 127.0.0.1:2346
    pub healthcheck_address: SocketAddr,
    /// Load actors from the bundle file (possibly generating it if it doesn't exist)
    pub load_actors: bool,
}

impl Default for Client {
    fn default() -> Self {
        let dir = ProjectDirs::from("com", "ChainSafe", "Forest").expect("failed to find project directories, please set FOREST_CONFIG_PATH environment variable manually.");
        Self {
            data_dir: dir.data_dir().to_path_buf(),
            genesis_file: None,
            enable_rpc: true,
            enable_metrics_endpoint: true,
            enable_health_check: true,
            snapshot_path: None,
            import_mode: ImportMode::default(),
            snapshot_height: None,
            snapshot_head: None,
            skip_load: false,
            chunk_size: ChunkSize::default(),
            buffer_size: BufferSize::default(),
            encrypt_keystore: true,
            metrics_address: FromStr::from_str("0.0.0.0:6116").unwrap(),
            rpc_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), crate::rpc::DEFAULT_PORT),
            rpc_filter_list: None,
            healthcheck_address: SocketAddr::new(
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                crate::health::DEFAULT_HEALTHCHECK_PORT,
            ),
            load_actors: true,
        }
    }
}

impl Client {
    pub fn default_rpc_token_path(&self) -> PathBuf {
        self.data_dir.join("token")
    }

    pub fn rpc_v1_endpoint(&self) -> Result<url::Url, url::ParseError> {
        format!("http://{}/rpc/v1", self.rpc_address)
            .as_str()
            .parse()
    }
}
