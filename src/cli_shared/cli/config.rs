// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::db::db_engine::DbConfig;
use crate::libp2p::Libp2pConfig;
use crate::shim::clock::ChainEpoch;
use crate::{chain_sync::SyncConfig, networks::NetworkChain};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::client::Client;

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
