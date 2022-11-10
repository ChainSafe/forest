// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_chain_sync::SyncConfig;
use forest_libp2p::Libp2pConfig;
use forest_networks::ChainConfig;
use log::LevelFilter;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use url::Url;

use super::client::Client;

#[derive(Serialize, Deserialize, PartialEq, Eq)]
pub struct LogConfig {
    pub filters: Vec<LogValue>,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            filters: vec![
                LogValue::new("libp2p_gossipsub", LevelFilter::Error),
                LogValue::new("filecoin_proofs", LevelFilter::Warn),
                LogValue::new("storage_proofs_core", LevelFilter::Warn),
                LogValue::new("surf::middleware", LevelFilter::Warn),
                LogValue::new("bellperson::groth16::aggregate::verify", LevelFilter::Warn),
                LogValue::new("tide", LevelFilter::Warn),
                LogValue::new("libp2p_bitswap", LevelFilter::Warn),
                LogValue::new("rpc", LevelFilter::Error),
            ],
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash)]
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

#[derive(Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SnapshotFetchConfig {
    pub mainnet: MainnetSnapshotFetchConfig,
    pub calibnet: CalibnetSnapshotFetchConfig,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
pub struct MainnetSnapshotFetchConfig {
    pub snapshot_url: Url,
}

impl Default for MainnetSnapshotFetchConfig {
    fn default() -> Self {
        // unfallible unwrap as we know that the value is correct
        Self {
            /// Default `mainnet` snapshot URL. The assumption is that it will redirect once and will contain a
            /// `sha256sum` file with the same URL (but different extension).
            snapshot_url: Url::try_from("https://fil-chain-snapshots-fallback.s3.amazonaws.com/mainnet/minimal_finality_stateroots_latest.car").unwrap(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
pub struct CalibnetSnapshotFetchConfig {
    pub snapshot_spaces_url: Url,
    pub bucket_name: String,
    pub region: String,
    pub path: String,
}

impl Default for CalibnetSnapshotFetchConfig {
    fn default() -> Self {
        // unfallible unwrap as we know that the value is correct
        Self {
            snapshot_spaces_url: Url::try_from(
                "https://forest-snapshots.fra1.digitaloceanspaces.com",
            )
            .unwrap(),
            bucket_name: "forest-snapshots".to_string(),
            region: "fra1".to_string(),
            path: "calibnet/".to_string(),
        }
    }
}

/// Structure that defines daemon configuration when process is detached
#[derive(Deserialize, Serialize, PartialEq, Eq)]
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

#[derive(Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Config {
    pub client: Client,
    pub rocks_db: forest_db::rocks_config::RocksDbConfig,
    pub network: Libp2pConfig,
    pub sync: SyncConfig,
    pub chain: Arc<ChainConfig>,
    pub daemon: DaemonConfig,
    pub log: LogConfig,
    pub snapshot_fetch: SnapshotFetchConfig,
}

#[cfg(test)]
mod test {
    use super::*;
    use forest_db::rocks_config::RocksDbConfig;
    use forest_utils::io::ProgressBarVisibility;
    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;
    use std::{
        net::{Ipv4Addr, SocketAddr},
        path::PathBuf,
    };

    /// Partial configuration, as some parts of the proper one don't implement required traits (i.e.
    /// Debug)
    #[derive(Clone, Debug)]
    struct ConfigPartial {
        client: Client,
        rocks_db: forest_db::rocks_config::RocksDbConfig,
        network: forest_libp2p::Libp2pConfig,
        sync: forest_chain_sync::SyncConfig,
    }

    impl From<ConfigPartial> for Config {
        fn from(val: ConfigPartial) -> Self {
            Config {
                client: val.client,
                rocks_db: val.rocks_db,
                network: val.network,
                sync: val.sync,
                chain: Arc::new(ChainConfig::default()),
                daemon: DaemonConfig::default(),
                log: Default::default(),
                snapshot_fetch: Default::default(),
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
                    rpc_port: u16::arbitrary(g),
                    rpc_token: Option::arbitrary(g),
                    snapshot: bool::arbitrary(g),
                    halt_after_import: bool::arbitrary(g),
                    snapshot_height: Option::arbitrary(g),
                    snapshot_path: Option::arbitrary(g),
                    skip_load: bool::arbitrary(g),
                    encrypt_keystore: bool::arbitrary(g),
                    metrics_address: SocketAddr::arbitrary(g),
                    rpc_address: SocketAddr::arbitrary(g),
                    download_snapshot: bool::arbitrary(g),
                    show_progress_bars: ProgressBarVisibility::arbitrary(g),
                },
                rocks_db: RocksDbConfig {
                    create_if_missing: bool::arbitrary(g),
                    parallelism: i32::arbitrary(g),
                    write_buffer_size: usize::arbitrary(g),
                    max_open_files: i32::arbitrary(g),
                    max_background_jobs: Option::arbitrary(g),
                    compaction_style: String::arbitrary(g),
                    compression_type: String::arbitrary(g),
                    enable_statistics: bool::arbitrary(g),
                    stats_dump_period_sec: u32::arbitrary(g),
                    log_level: String::arbitrary(g),
                    optimize_filters_for_hits: bool::arbitrary(g),
                    optimize_for_point_lookup: i32::arbitrary(g),
                },
                network: Libp2pConfig {
                    listening_multiaddr: Ipv4Addr::arbitrary(g).into(),
                    bootstrap_peers: vec![Ipv4Addr::arbitrary(g).into(); u8::arbitrary(g) as usize],
                    mdns: bool::arbitrary(g),
                    kademlia: bool::arbitrary(g),
                    target_peer_count: u32::arbitrary(g),
                },
                sync: SyncConfig {
                    req_window: i64::arbitrary(g),
                    tipset_sample_size: usize::arbitrary(g),
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
}
