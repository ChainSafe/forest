// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(clippy::too_many_arguments)]

#[cfg(all(f3sidecar, not(feature = "no-f3-sidecar")))]
mod go_ffi;
use std::path::{Path, PathBuf};

#[cfg(all(f3sidecar, not(feature = "no-f3-sidecar")))]
use go_ffi::*;

pub mod snapshot;

use cid::Cid;

use crate::{networks::ChainConfig, utils::misc::env::is_env_set_and_truthy};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct `F3`Options {
    pub chain_finality: i64,
    pub bootstrap_epoch: i64,
    pub initial_power_table: Option<Cid>,
}

pub fn get_f3_root(config: &crate::Config) -> PathBuf {
    std::env::var("FOREST_`F3`_ROOT")
        .map(|p| Path::new(&p).to_path_buf())
        .unwrap_or_else(|_| {
            config
                .client
                .data_dir
                .join(format!("f3/{}", config.chain()))
        })
}

pub fn get_f3_sidecar_params(chain_config: &ChainConfig) -> `F3`Options {
    let chain_finality = std::env::var("FOREST_`F3`_FINALITY")
        .ok()
        .and_then(|v| match v.parse::<i64>() {
            Ok(f) if f > 0 => Some(f),
            _ => {
                tracing::warn!(
                    "Invalid FOREST_`F3`_FINALITY value {v}. A positive integer is expected."
                );
                None
            }
        })
        .inspect(|i| {
            tracing::info!("Using `F3` finality {i} set by FOREST_`F3`_FINALITY");
        })
        .unwrap_or(chain_config.policy.chain_finality);
    // This will be used post-bootstrap to hard-code the initial `F3`'s initial power table CID.
    // Read from an environment variable for now before the hard-coded value is determined.
    let initial_power_table = match std::env::var("FOREST_`F3`_INITIAL_POWER_TABLE") {
        Ok(i) if i.is_empty() => {
            tracing::info!("`F3` initial power table cid is unset by FOREST_`F3`_INITIAL_POWER_TABLE");
            None
        }
        Ok(i) => {
            if let Ok(cid) = i.parse() {
                tracing::info!(
                    "Using `F3` initial power table cid {i} set by FOREST_`F3`_INITIAL_POWER_TABLE"
                );
                Some(cid)
            } else {
                tracing::warn!(
                    "Invalid power table cid {i} set by FOREST_`F3`_INITIAL_POWER_TABLE, fallback to chain config"
                );
                chain_config.f3_initial_power_table
            }
        }
        _ => chain_config.f3_initial_power_table,
    };

    let bootstrap_epoch = std::env::var("FOREST_`F3`_BOOTSTRAP_EPOCH")
        .ok()
        .and_then(|i| i.parse().ok())
        .inspect(|i| {
            tracing::info!("Using `F3` bootstrap epoch {i} set by FOREST_`F3`_BOOTSTRAP_EPOCH")
        })
        .unwrap_or(chain_config.f3_bootstrap_epoch);

    `F3`Options {
        chain_finality,
        bootstrap_epoch,
        initial_power_table,
    }
}

#[allow(unused_variables)]
pub fn run_f3_sidecar_if_enabled(
    chain_config: &ChainConfig,
    rpc_endpoint: String,
    jwt: String,
    f3_rpc_endpoint: String,
    initial_power_table: String,
    bootstrap_epoch: i64,
    finality: i64,
    f3_root: String,
) {
    if is_sidecar_ffi_enabled(chain_config) {
        #[cfg(all(f3sidecar, not(feature = "no-f3-sidecar")))]
        {
            tracing::info!("Starting `F3` sidecar service ...");
            Go`F3`NodeImpl::run(
                rpc_endpoint,
                jwt,
                f3_rpc_endpoint,
                initial_power_table,
                bootstrap_epoch,
                finality,
                f3_root,
            );
        }
    }
}

#[allow(unused_variables)]
pub fn import_f3_snapshot(
    chain_config: &ChainConfig,
    rpc_endpoint: String,
    f3_root: String,
    snapshot: String,
) -> anyhow::Result<()> {
    if is_sidecar_ffi_enabled(chain_config) {
        #[cfg(all(f3sidecar, not(feature = "no-f3-sidecar")))]
        {
            let sw = std::time::Instant::now();
            tracing::info!("Importing `F3` snapshot ...");
            let err = Go`F3`NodeImpl::import_snap(rpc_endpoint, f3_root, snapshot);
            if !err.is_empty() {
                anyhow::bail!("{err}");
            }
            tracing::info!(
                "Imported `F3` snapshot, took {}",
                humantime::format_duration(sw.elapsed())
            );
        }
    } else {
        tracing::warn!("`F3` sidecar is disabled, skip importing the `F3` snapshot");
    }
    Ok(())
}

/// Whether `F3` sidecar via FFI is enabled.
pub fn is_sidecar_ffi_enabled(chain_config: &ChainConfig) -> bool {
    // Respect the environment variable when set, and fallback to chain config when not set.
    let enabled =
        is_env_set_and_truthy("FOREST_`F3`_SIDECAR_FFI_ENABLED").unwrap_or(chain_config.f3_enabled);
    cfg_if::cfg_if! {
        if #[cfg(all(f3sidecar, not(feature = "no-f3-sidecar")))] {
            enabled
        }
        else {
            if enabled {
                tracing::info!("Failed to enable `F3` sidecar, the Forest binary is not compiled with f3-sidecar Go lib");
            }
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_f3_sidecar_params() {
        let chain_config = ChainConfig::calibnet();
        // No environment variable overrides
        assert_eq!(
            get_f3_sidecar_params(&chain_config),
            `F3`Options {
                chain_finality: chain_config.policy.chain_finality,
                bootstrap_epoch: chain_config.f3_bootstrap_epoch,
                initial_power_table: chain_config.f3_initial_power_table,
            }
        );

        unsafe {
            std::env::set_var("FOREST_`F3`_FINALITY", "100");
            // A random CID
            std::env::set_var(
                "FOREST_`F3`_INITIAL_POWER_TABLE",
                "bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i",
            );
            std::env::set_var("FOREST_`F3`_BOOTSTRAP_EPOCH", "100");
        }
        assert_eq!(
            get_f3_sidecar_params(&chain_config),
            `F3`Options {
                chain_finality: 100,
                bootstrap_epoch: 100,
                initial_power_table: Some(
                    "bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i"
                        .parse()
                        .unwrap()
                ),
            }
        );
        // Unset initial power table
        unsafe { std::env::set_var("FOREST_`F3`_INITIAL_POWER_TABLE", "") };
        assert_eq!(
            get_f3_sidecar_params(&chain_config),
            `F3`Options {
                chain_finality: 100,
                bootstrap_epoch: 100,
                initial_power_table: None,
            }
        );
    }
}
