// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod client;
mod config;
mod snapshot_fetch;

pub use self::{client::*, config::*, snapshot_fetch::*};
use crate::logger::LoggingColor;

use byte_unit::Byte;
use directories::ProjectDirs;
use forest_networks::ChainConfig;
use forest_utils::io::{read_file_to_string, read_toml};
use fvm_shared::bigint::BigInt;
use git_version::git_version;
use log::{error, info, warn};
use once_cell::sync::Lazy;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use structopt::StructOpt;

const GIT_HASH: &str = git_version!(args = ["--always", "--exclude", "*"], fallback = "unknown");

pub static FOREST_VERSION_STRING: Lazy<String> =
    Lazy::new(|| format!("{}+git.{}", env!("CARGO_PKG_VERSION"), GIT_HASH));

/// CLI options
#[derive(StructOpt, Debug)]
pub struct CliOpts {
    /// A TOML file containing relevant configurations
    #[structopt(short, long)]
    pub config: Option<String>,
    /// The genesis CAR file
    #[structopt(short, long)]
    pub genesis: Option<String>,
    /// Allow RPC to be active or not (default: true)
    #[structopt(short, long)]
    pub rpc: Option<bool>,
    /// Client JWT token to use for JSON-RPC authentication
    #[structopt(short, long)]
    pub token: Option<String>,
    /// Address used for metrics collection server. By defaults binds on localhost on port 6116.
    #[structopt(long)]
    pub metrics_address: Option<SocketAddr>,
    /// Address used for RPC. By defaults binds on localhost on port 1234.
    #[structopt(long)]
    pub rpc_address: Option<SocketAddr>,
    /// Allow Kademlia (default: true)
    #[structopt(short, long)]
    pub kademlia: Option<bool>,
    /// Allow MDNS (default: false)
    #[structopt(long)]
    pub mdns: Option<bool>,
    /// Validate snapshot at given EPOCH
    #[structopt(long)]
    pub height: Option<i64>,
    /// Import a snapshot from a local CAR file or URL
    #[structopt(long)]
    pub import_snapshot: Option<String>,
    /// Halt with exit code 0 after successfully importing a snapshot
    #[structopt(long)]
    pub halt_after_import: bool,
    /// Import a chain from a local CAR file or URL
    #[structopt(long)]
    pub import_chain: Option<String>,
    /// Skips loading CAR file and uses header to index chain. Assumes a pre-loaded database
    #[structopt(long)]
    pub skip_load: bool,
    /// Number of tipsets requested over chain exchange (default is 200)
    #[structopt(long)]
    pub req_window: Option<i64>,
    /// Number of tipsets to include in the sample that determines what the network head is
    #[structopt(long)]
    pub tipset_sample_size: Option<u8>,
    /// Amount of Peers we want to be connected to (default is 75)
    #[structopt(long)]
    pub target_peer_count: Option<u32>,
    /// Encrypt the key-store (default: true)
    #[structopt(long)]
    pub encrypt_keystore: Option<bool>,
    /// Choose network chain to sync to
    #[structopt(
        long,
        default_value = "mainnet",
        possible_values = &["mainnet", "calibnet"],
    )]
    pub chain: String,
    /// Daemonize Forest process
    #[structopt(long)]
    pub detach: bool,
    /// Download a chain specific snapshot to sync with the Filecoin network
    #[structopt(long)]
    pub download_snapshot: bool,
    /// Enable or disable colored logging in `stdout`
    #[structopt(long, default_value = "auto")]
    pub color: LoggingColor,
    // env_logger-0.7 can only redirect to stderr or stdout. Version 0.9 can redirect to a file.
    // However, we cannot upgrade to version 0.9 because pretty_env_logger depends on version 0.7
    // and hasn't been updated in quite a while. See https://github.com/seanmonstar/pretty-env-logger/issues/52
    // #[structopt(
    //     help = "Specify a filename into which logging should be appended"
    // )]
    // pub log_file: Option<PathBuf>,
}

impl CliOpts {
    pub fn to_config(&self) -> Result<Config, io::Error> {
        let mut cfg: Config = match &self.config {
            Some(config_file) => {
                // Read from config file
                let toml = read_file_to_string(&PathBuf::from(&config_file))?;
                // Parse and return the configuration file
                read_toml(&toml)?
            }
            None => find_default_config().unwrap_or_default(),
        };

        if self.chain == "calibnet" {
            // override the chain configuration
            cfg.chain = Arc::new(ChainConfig::calibnet());
        }

        if let Some(genesis_file) = &self.genesis {
            cfg.client.genesis_file = Some(genesis_file.to_owned());
        }
        if self.rpc.unwrap_or(cfg.client.enable_rpc) {
            cfg.client.enable_rpc = true;
            if let Some(rpc_address) = self.rpc_address {
                cfg.client.rpc_address = rpc_address;
            }

            if self.token.is_some() {
                cfg.client.rpc_token = self.token.to_owned();
            }
        } else {
            cfg.client.enable_rpc = false;
        }
        if let Some(metrics_address) = self.metrics_address {
            cfg.client.metrics_address = metrics_address;
        }
        if self.import_snapshot.is_some() && self.import_chain.is_some() {
            cli_error_and_die(
                "Can't set import_snapshot and import_chain at the same time!",
                1,
            );
        } else {
            if let Some(snapshot_path) = &self.import_snapshot {
                cfg.client.snapshot_path = Some(snapshot_path.into());
                cfg.client.snapshot = true;
            }
            if let Some(snapshot_path) = &self.import_chain {
                cfg.client.snapshot_path = Some(snapshot_path.into());
                cfg.client.snapshot = false;
            }
            cfg.client.snapshot_height = self.height;

            cfg.client.skip_load = self.skip_load;
        }

        cfg.client.halt_after_import = self.halt_after_import;
        cfg.client.download_snapshot = self.download_snapshot;

        cfg.network.kademlia = self.kademlia.unwrap_or(cfg.network.kademlia);
        cfg.network.mdns = self.mdns.unwrap_or(cfg.network.mdns);
        if let Some(target_peer_count) = self.target_peer_count {
            cfg.network.target_peer_count = target_peer_count;
        }
        // (where to find these flags, should be easy to do with structops)

        // check and set syncing configurations
        // TODO add MAX conditions
        if let Some(req_window) = &self.req_window {
            cfg.sync.req_window = req_window.to_owned();
        }
        if let Some(tipset_sample_size) = self.tipset_sample_size {
            cfg.sync.tipset_sample_size = tipset_sample_size.into();
        }
        if let Some(encrypt_keystore) = self.encrypt_keystore {
            cfg.client.encrypt_keystore = encrypt_keystore;
        }

        Ok(cfg)
    }
}

fn find_default_config() -> Option<Config> {
    if let Ok(config_file) = std::env::var("FOREST_CONFIG_PATH") {
        info!(
            "FOREST_CONFIG_PATH detected, using configuration at {}",
            config_file
        );
        let path = PathBuf::from(config_file);
        if path.exists() {
            return read_config_or_none(path);
        }
    };

    if let Some(dir) = ProjectDirs::from("com", "ChainSafe", "Forest") {
        let mut config_dir = dir.config_dir().to_path_buf();
        config_dir.push("config.toml");
        if config_dir.exists() {
            info!("Found a config file at {}", config_dir.display());
            return read_config_or_none(config_dir);
        }
    }

    warn!("No configurations found, using defaults.");

    None
}

fn read_config_or_none(path: PathBuf) -> Option<Config> {
    let toml = match read_file_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            warn!("An error occured while reading configuration file at {}. Resorting to default configuration. Error was {}", path.display(), e);
            return None;
        }
    };

    match read_toml(&toml) {
        Ok(cfg) => Some(cfg),
        Err(e) => {
            warn!(
                "Error reading configuration file, opting to defaults. Error was {} ",
                e
            );
            None
        }
    }
}

pub fn default_snapshot_dir(config: &Config) -> PathBuf {
    config
        .client
        .data_dir
        .join("snapshots")
        .join(config.chain.name.clone())
}

/// Print an error message and exit the program with an error code
/// Used for handling high level errors such as invalid parameters
pub fn cli_error_and_die(msg: impl AsRef<str>, code: i32) -> ! {
    error!("Error: {}", msg.as_ref());
    std::process::exit(code);
}

/// convert `BigInt` to size string using byte size units (i.e. KiB, GiB, PiB, etc)
/// Provided number cannot be negative, otherwise the function will panic.
pub fn to_size_string(input: &BigInt) -> anyhow::Result<String> {
    let bytes = u128::try_from(input)
        .map_err(|e| anyhow::anyhow!("error parsing the input {}: {}", input, e))?;

    Ok(Byte::from_bytes(bytes)
        .get_appropriate_unit(true)
        .to_string())
}

#[cfg(test)]
mod test {
    use super::*;
    use fvm_shared::bigint::Zero;

    #[test]
    fn to_size_string_valid_input() {
        let cases = [
            (BigInt::zero(), "0 B"),
            (BigInt::from(1 << 10), "1024 B"),
            (BigInt::from((1 << 10) + 1), "1.00 KiB"),
            (BigInt::from((1 << 10) + 512), "1.50 KiB"),
            (BigInt::from(1 << 20), "1024.00 KiB"),
            (BigInt::from((1 << 20) + 1), "1.00 MiB"),
            (BigInt::from(1 << 29), "512.00 MiB"),
            (BigInt::from((1 << 30) + 1), "1.00 GiB"),
            (BigInt::from((1u64 << 40) + 1), "1.00 TiB"),
            (BigInt::from((1u64 << 50) + 1), "1.00 PiB"),
            // ZiB is 2^70, 288230376151711744 is 2^58
            (BigInt::from(u128::MAX), "288230376151711744.00 ZiB"),
        ];

        for (input, expected) in cases {
            assert_eq!(to_size_string(&input).unwrap(), expected.to_string());
        }
    }

    #[test]
    fn to_size_string_negative_input_should_fail() {
        assert!(to_size_string(&BigInt::from(-1i8)).is_err());
    }

    #[test]
    fn to_size_string_too_large_input_should_fail() {
        assert!(to_size_string(&(BigInt::from(u128::MAX) + 1)).is_err());
    }
}
