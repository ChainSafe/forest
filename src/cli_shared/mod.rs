// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod cli;
pub mod logger;

use crate::cli_shared::cli::{find_config_path, Config, ConfigPath};
use crate::networks::NetworkChain;
use crate::utils::io::{read_file_to_string, read_toml};
use std::path::PathBuf;

#[cfg(feature = "mimalloc")]
pub use mimalloc;
#[cfg(feature = "jemalloc")]
pub use tikv_jemallocator;

/// Gets chain data directory
pub fn chain_path(config: &Config) -> PathBuf {
    PathBuf::from(&config.client.data_dir).join(config.chain.to_string())
}

pub fn read_config(
    config_path_opt: &Option<String>,
    chain_opt: &Option<NetworkChain>,
) -> anyhow::Result<(Option<ConfigPath>, Config)> {
    let (path, mut config) = match find_config_path(config_path_opt) {
        Some(path) => {
            // Read from config file
            let toml = read_file_to_string(path.to_path_buf())?;
            // Parse and return the configuration file
            (Some(path), read_toml(&toml)?)
        }
        None => (None, Config::default()),
    };
    if let Some(chain) = chain_opt {
        config.chain = chain.clone();
    }
    Ok((path, config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_config_default() {
        let (config_path, config) = read_config(&None, &None).unwrap();

        assert!(config_path.is_none());
        assert_eq!(config.chain, NetworkChain::Mainnet);
    }

    #[test]
    fn read_config_calibnet_override() {
        let (config_path, config) = read_config(&None, &Some(NetworkChain::Calibnet)).unwrap();

        assert!(config_path.is_none());
        assert_eq!(config.chain, NetworkChain::Calibnet);
    }

    #[test]
    fn read_config_with_path() {
        let default_config = Config::default();
        let path: PathBuf = "config.toml".into();
        let serialized_config = toml::to_string(&default_config).unwrap();
        std::fs::write(path.clone(), serialized_config).unwrap();

        let (config_path, config) =
            read_config(&Some(path.to_str().unwrap().into()), &None).unwrap();

        assert_eq!(config_path.unwrap(), ConfigPath::Cli(path));
        assert_eq!(config.chain, NetworkChain::Mainnet);
        assert_eq!(config, default_config);
    }
}

pub mod snapshot;
