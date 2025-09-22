// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::client::Client;
use crate::db::db_engine::DbConfig;
use crate::libp2p::Libp2pConfig;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use crate::utils::misc::env::is_env_set_and_truthy;
use crate::{chain_sync::SyncConfig, networks::NetworkChain};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;

const FOREST_CHAIN_INDEXER_ENABLED: &str = "FOREST_CHAIN_INDEXER_ENABLED";

/// Structure that defines daemon configuration when process is detached
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
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

/// Structure that defines events configuration
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct EventsConfig {
    #[cfg_attr(test, arbitrary(gen(|g| u32::arbitrary(g) as _)))]
    pub max_filter_results: usize,
    pub max_filter_height_range: ChainEpoch,
}

impl Default for EventsConfig {
    fn default() -> Self {
        Self {
            max_filter_results: 10000,
            max_filter_height_range: 2880,
        }
    }
}

/// Structure that defines `FEVM` configuration
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct FevmConfig {
    #[cfg_attr(test, arbitrary(gen(|g| u32::arbitrary(g) as _)))]
    pub eth_trace_filter_max_results: usize,
}

impl Default for FevmConfig {
    fn default() -> Self {
        Self {
            eth_trace_filter_max_results: 500,
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct ChainIndexerConfig {
    /// Enable indexing Ethereum mappings
    pub enable_indexer: bool,
    /// Number of retention epochs for indexed entries. Set to `None` to disable garbage collection.
    pub gc_retention_epochs: Option<u32>,
}

impl Default for ChainIndexerConfig {
    fn default() -> Self {
        Self {
            enable_indexer: is_env_set_and_truthy(FOREST_CHAIN_INDEXER_ENABLED).unwrap_or(false),
            gc_retention_epochs: None,
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct FeeConfig {
    pub max_fee: TokenAmount,
}

impl Default for FeeConfig {
    fn default() -> Self {
        // This indicates the default max fee for a message,
        // The code is taken from https://github.com/filecoin-project/lotus/blob/release/v1.34.1/node/config/def.go#L39
        Self {
            max_fee: TokenAmount::from_atto(
                num_bigint::BigInt::from_str("70000000000000000").unwrap(),
            ), // 0.07 FIL
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Default, Debug, Clone)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[serde(default)]
pub struct Config {
    pub chain: NetworkChain,
    pub client: Client,
    pub parity_db: crate::db::parity_db_config::ParityDbConfig,
    pub network: Libp2pConfig,
    pub sync: SyncConfig,
    pub daemon: DaemonConfig,
    pub events: EventsConfig,
    pub fevm: FevmConfig,
    pub fee: FeeConfig,
    pub chain_indexer: ChainIndexerConfig,
}

impl Config {
    pub fn db_config(&self) -> &DbConfig {
        &self.parity_db
    }

    pub fn chain(&self) -> &NetworkChain {
        &self.chain
    }
}

#[cfg(test)]
mod test {
    use quickcheck_macros::quickcheck;

    use super::*;

    #[quickcheck]
    fn test_config_all_params_under_section(config: Config) {
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
