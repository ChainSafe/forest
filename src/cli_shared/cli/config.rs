// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use core::time::Duration;
use std::{path::PathBuf, sync::Arc};

use crate::chain_sync::SyncConfig;
use crate::db::db_engine::DbConfig;
use crate::libp2p::Libp2pConfig;
use crate::networks::ChainConfig;
use log::LevelFilter;
use serde::{Deserialize, Serialize};

use super::client::Client;

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone)]
pub struct LogConfig {
    pub filters: Vec<LogValue>,
}

impl LogConfig {
    pub(in crate::cli_shared) fn to_filter_string(&self) -> String {
        self.filters
            .iter()
            .map(|f| format!("{}={}", f.module, f.level))
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            filters: vec![
                LogValue::new("axum", LevelFilter::Warn),
                LogValue::new("bellperson::groth16::aggregate::verify", LevelFilter::Warn),
                LogValue::new("filecoin_proofs", LevelFilter::Warn),
                LogValue::new("libp2p_bitswap", LevelFilter::Off),
                LogValue::new("libp2p_gossipsub", LevelFilter::Error),
                LogValue::new("libp2p_kad", LevelFilter::Error),
                LogValue::new("rpc", LevelFilter::Error),
                LogValue::new("storage_proofs_core", LevelFilter::Warn),
                LogValue::new("tracing_loki", LevelFilter::Off),
            ],
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone)]
pub struct LogValue {
    pub module: String,
    pub level: LevelFilter,
}

impl LogValue {
    pub fn new(module: &str, level: LevelFilter) -> Self {
        Self {
            module: module.to_string(),
            level,
        }
    }
}

/// Structure that defines daemon configuration when process is detached
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct DaemonConfig {
    pub user: Option<String>,
    pub group: Option<String>,
    pub umask: u16,
    pub stdout: PathBuf,
    pub stderr: PathBuf,
    pub work_dir: PathBuf,
    pub pid_file: Option<PathBuf>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            user: None,
            group: None,
            umask: 0o027,
            stdout: "forest.out".into(),
            stderr: "forest.err".into(),
            work_dir: ".".into(),
            pid_file: None,
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Clone, Default, Debug)]
pub struct TokioConfig {
    pub worker_threads: Option<usize>,
    pub max_blocking_threads: Option<usize>,
    pub thread_keep_alive: Option<Duration>,
    pub thread_stack_size: Option<usize>,
    pub global_queue_interval: Option<u32>,
}

#[derive(Serialize, Deserialize, PartialEq, Default, Debug, Clone)]
#[serde(default)]
pub struct Config {
    pub client: Client,
    pub parity_db: crate::db::parity_db_config::ParityDbConfig,
    pub network: Libp2pConfig,
    pub sync: SyncConfig,
    pub chain: Arc<ChainConfig>,
    pub daemon: DaemonConfig,
    pub log: LogConfig,
    pub tokio: TokioConfig,
}

impl Config {
    pub fn db_config(&self) -> &DbConfig {
        &self.parity_db
    }
}

#[cfg(test)]
mod test {
    use std::{
        net::{Ipv4Addr, SocketAddr},
        path::PathBuf,
    };

    use crate::cli_shared::cli::client::{BufferSize, ChunkSize};
    use crate::utils::io::ProgressBarVisibility;
    use chrono::Duration;
    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;
    use tracing_subscriber::EnvFilter;

    use super::*;

    /// Partial configuration, as some parts of the proper one don't implement
    /// required traits (i.e. Debug)
    // This should be removed in #2965
    #[derive(Clone, Debug)]
    struct ConfigPartial {
        client: Client,
        parity_db: crate::db::parity_db_config::ParityDbConfig,
        network: crate::libp2p::Libp2pConfig,
        sync: crate::chain_sync::SyncConfig,
    }

    impl From<ConfigPartial> for Config {
        fn from(val: ConfigPartial) -> Self {
            Config {
                client: val.client,
                parity_db: val.parity_db,
                network: val.network,
                sync: val.sync,
                chain: Arc::new(ChainConfig::default()),
                daemon: DaemonConfig::default(),
                log: Default::default(),
                tokio: Default::default(),
            }
        }
    }

    impl Arbitrary for ConfigPartial {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            ConfigPartial {
                client: Client {
                    data_dir: PathBuf::arbitrary(g),
                    genesis_file: Option::arbitrary(g),
                    enable_rpc: bool::arbitrary(g),
                    rpc_token: Option::arbitrary(g),
                    snapshot: bool::arbitrary(g),
                    snapshot_height: Option::arbitrary(g),
                    snapshot_path: Option::arbitrary(g),
                    snapshot_head: Option::arbitrary(g),
                    skip_load: bool::arbitrary(g),
                    chunk_size: ChunkSize::arbitrary(g),
                    buffer_size: BufferSize::arbitrary(g),
                    encrypt_keystore: bool::arbitrary(g),
                    metrics_address: SocketAddr::arbitrary(g),
                    rpc_address: SocketAddr::arbitrary(g),
                    token_exp: Duration::milliseconds(i64::arbitrary(g)),
                    show_progress_bars: ProgressBarVisibility::arbitrary(g),
                },
                parity_db: crate::db::parity_db_config::ParityDbConfig {
                    enable_statistics: bool::arbitrary(g),
                    compression_type: String::arbitrary(g),
                },
                network: Libp2pConfig {
                    listening_multiaddrs: vec![Ipv4Addr::arbitrary(g).into()],
                    bootstrap_peers: vec![Ipv4Addr::arbitrary(g).into(); u8::arbitrary(g) as usize],
                    mdns: bool::arbitrary(g),
                    kademlia: bool::arbitrary(g),
                    target_peer_count: u32::arbitrary(g),
                },
                sync: SyncConfig {
                    req_window: i64::arbitrary(g),
                    tipset_sample_size: u32::arbitrary(g) as _,
                },
            }
        }
    }

    #[quickcheck]
    fn test_config_all_params_under_section(config: ConfigPartial) {
        let config = Config::from(config);
        let serialized_config =
            toml::to_string(&config).expect("could not serialize the configuration");
        assert_eq!(
            serialized_config
                .trim_start()
                .chars()
                .next()
                .expect("configuration empty"),
            '['
        )
    }

    #[test]
    fn test_default_log_filters() {
        let config = LogConfig::default();
        EnvFilter::builder()
            .parse(config.to_filter_string())
            .unwrap();
    }
}
