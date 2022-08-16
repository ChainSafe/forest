// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_chain_sync::SyncConfig;
use forest_libp2p::Libp2pConfig;
use forest_networks::ChainConfig;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::client::Client;

#[derive(Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Config {
    pub client: Client,
    pub rocks_db: forest_db::rocks_config::RocksDbConfig,
    pub network: Libp2pConfig,
    pub sync: SyncConfig,
    pub chain: Arc<ChainConfig>,
}

#[cfg(test)]
mod test {
    use super::*;
    use forest_db::rocks_config::RocksDbConfig;
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
                    snapshot_height: Option::arbitrary(g),
                    snapshot_path: Option::arbitrary(g),
                    skip_load: bool::arbitrary(g),
                    encrypt_keystore: bool::arbitrary(g),
                    metrics_address: SocketAddr::arbitrary(g),
                    rpc_address: SocketAddr::arbitrary(g),
                },
                rocks_db: RocksDbConfig {
                    create_if_missing: bool::arbitrary(g),
                    parallelism: i32::arbitrary(g),
                    write_buffer_size: usize::arbitrary(g),
                    max_open_files: i32::arbitrary(g),
                    max_background_jobs: Option::arbitrary(g),
                    compression_type: Option::arbitrary(g),
                    compaction_style: Option::arbitrary(g),
                    enable_statistics: bool::arbitrary(g),
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
