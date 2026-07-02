// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod cli;
pub mod logger;

use crate::cli_shared::cli::{Config, ConfigPath, find_config_path};
use crate::networks::NetworkChain;
use crate::utils::io::read_toml;
use std::path::PathBuf;

cfg_if::cfg_if! {
    if #[cfg(feature = "rustalloc")] {
    } else if #[cfg(feature = "jemalloc")] {
        pub use tikv_jemallocator;
    }
}

/// Environment variable that overrides the Forest data directory, taking
/// precedence over both the configuration file and the built-in default. Named
/// after Lotus' `LOTUS_PATH` to ease switching between implementations.
pub const FOREST_DATA_DIR_ENV: &str = "FOREST_PATH";

/// Gets chain data directory
pub fn chain_path(config: &Config) -> PathBuf {
    PathBuf::from(&config.client.data_dir).join(config.chain().to_string())
}

/// Deletes the chain database and F3 data for the configured chain.
pub fn delete_chain_data(config: &Config) -> anyhow::Result<()> {
    for dir in [chain_path(config), crate::f3::get_f3_root(config)] {
        if dir.is_dir() {
            tracing::info!("Removing {}", dir.display());
            fs_extra::dir::remove(&dir)
                .map_err(|e| anyhow::anyhow!("failed to remove {}: {e}", dir.display()))?;
        }
    }
    Ok(())
}

pub fn read_config(
    config_path_opt: Option<&PathBuf>,
    chain_opt: Option<NetworkChain>,
) -> anyhow::Result<(Option<ConfigPath>, Config)> {
    let (path, mut config) = match find_config_path(config_path_opt) {
        Some(path) => {
            // Read from config file
            let toml = std::fs::read_to_string(path.to_path_buf())?;
            // Parse and return the configuration file
            (Some(path), read_toml(&toml)?)
        }
        None => (None, Config::default()),
    };
    if let Some(chain) = chain_opt {
        config.chain = chain;
    }
    // The `FOREST_PATH` environment variable takes precedence over the data
    // directory set in the configuration file (or the default one).
    if let Some(data_dir) = data_dir_from_env() {
        config.client.data_dir = data_dir;
    }
    Ok((path, config))
}

/// Returns the data directory set via the [`FOREST_DATA_DIR_ENV`] environment
/// variable, if it is present and non-empty.
fn data_dir_from_env() -> Option<PathBuf> {
    match std::env::var(FOREST_DATA_DIR_ENV) {
        Ok(s) if !s.trim().is_empty() => Some(PathBuf::from(s)),
        _ => None,
    }
}

/// Returns the effective Forest data directory: the [`FOREST_DATA_DIR_ENV`]
/// environment variable if set, otherwise the built-in default. Unlike
/// [`read_config`], this does not consult a configuration file and is meant for
/// contexts (e.g. the RPC client) that need the data directory without loading
/// the full configuration.
pub fn default_data_dir() -> PathBuf {
    data_dir_from_env().unwrap_or_else(|| crate::cli_shared::cli::Client::default().data_dir)
}

/// Returns the path to the RPC admin token within the effective data directory
/// (see [`default_data_dir`]). This is where a daemon started with the same
/// environment saves the token, so clients can read it back from here.
pub fn default_token_path() -> PathBuf {
    default_data_dir().join(crate::cli_shared::cli::Client::RPC_TOKEN_FILENAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_chain_data_removes_chain_and_f3_but_keeps_keystore() {
        let tmp = tempfile::TempDir::new().unwrap();

        let mut config = Config::default();
        config.client.data_dir = tmp.path().to_path_buf();
        config.chain = NetworkChain::Calibnet;

        // Lay out a representative data directory: a versioned database, F3
        // sidecar data, and a wallet keystore.
        let chain_dir = chain_path(&config);
        let f3_dir = crate::f3::get_f3_root(&config);
        let keystore = config.client.data_dir.join(crate::KEYSTORE_NAME);
        std::fs::create_dir_all(chain_dir.join("0.1.0")).unwrap();
        std::fs::create_dir_all(&f3_dir).unwrap();
        std::fs::write(&keystore, b"wallet-keys").unwrap();

        delete_chain_data(&config).unwrap();

        assert!(!chain_dir.exists(), "chain database should be deleted");
        assert!(!f3_dir.exists(), "F3 data should be deleted");
        assert!(keystore.exists(), "keystore must be preserved");
    }

    #[test]
    fn read_config_default() {
        let (config_path, config) = read_config(None, None).unwrap();

        assert!(config_path.is_none());
        assert_eq!(config.chain(), &NetworkChain::Mainnet);
    }

    #[test]
    fn read_config_calibnet_override() {
        let (config_path, config) = read_config(None, Some(NetworkChain::Calibnet)).unwrap();

        assert!(config_path.is_none());
        assert_eq!(config.chain(), &NetworkChain::Calibnet);
    }

    #[test]
    fn read_config_butterflynet_override() {
        let (config_path, config) = read_config(None, Some(NetworkChain::Butterflynet)).unwrap();

        assert!(config_path.is_none());
        assert_eq!(config.chain(), &NetworkChain::Butterflynet);
    }

    /// Runs `f` with [`FOREST_DATA_DIR_ENV`] set to `value`, restoring the
    /// environment afterwards.
    fn with_data_dir_env<T>(value: &str, f: impl FnOnce() -> T) -> T {
        unsafe { std::env::set_var(FOREST_DATA_DIR_ENV, value) };
        let result = f();
        unsafe { std::env::remove_var(FOREST_DATA_DIR_ENV) };
        result
    }

    #[test]
    #[serial_test::serial]
    fn read_config_data_dir_env_override() {
        let data_dir = "/tmp/forest-path-env-override-test";
        let (_, config) = with_data_dir_env(data_dir, || read_config(None, None).unwrap());

        // The env variable takes precedence over the default data directory.
        assert_eq!(config.client.data_dir, std::path::Path::new(data_dir));
    }

    #[test]
    #[serial_test::serial]
    fn default_data_dir_honors_env_override() {
        let data_dir = "/tmp/forest-path-default-data-dir-test";
        let resolved = with_data_dir_env(data_dir, default_data_dir);
        assert_eq!(resolved, std::path::Path::new(data_dir));

        // Without the env variable, it falls back to the default data directory.
        assert_eq!(default_data_dir(), Config::default().client.data_dir);
    }

    #[test]
    #[serial_test::serial]
    fn read_config_data_dir_env_empty_is_ignored() {
        let (_, config) = with_data_dir_env("", || read_config(None, None).unwrap());

        // An empty env variable falls back to the default data directory.
        assert_eq!(config.client.data_dir, Config::default().client.data_dir);
    }

    #[test]
    #[serial_test::serial]
    fn read_config_with_path() {
        let default_config = Config::default();
        let temp_dir = tempfile::tempdir().expect("couldn't create temp dir");
        let config_file = temp_dir.path().join("config.toml");
        let serialized_config = toml::to_string(&default_config).unwrap();
        std::fs::write(&config_file, serialized_config).unwrap();

        let (config_path, config) = read_config(Some(&config_file), None).unwrap();

        assert_eq!(config_path.unwrap(), ConfigPath::Cli(config_file));
        assert_eq!(config.chain(), &NetworkChain::Mainnet);
        assert_eq!(config, default_config);
    }
}

pub mod snapshot;
