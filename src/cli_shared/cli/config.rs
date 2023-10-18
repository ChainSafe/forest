// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::chain_sync::SyncConfig;
use crate::db::db_engine::DbConfig;
use crate::libp2p::Libp2pConfig;
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

#[derive(Serialize, Deserialize, PartialEq, Default, Debug, Clone)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[serde(default)]
pub struct Config {
    pub client: Client,
    pub parity_db: crate::db::parity_db_config::ParityDbConfig,
    pub network: Libp2pConfig,
    pub sync: SyncConfig,
    pub daemon: DaemonConfig,
}

impl Config {
    pub fn db_config(&self) -> &DbConfig {
        &self.parity_db
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
