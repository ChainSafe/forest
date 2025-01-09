// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(clippy::too_many_arguments)]

#[cfg(all(f3sidecar, not(feature = "no-f3-sidecar")))]
mod go_ffi;
#[cfg(all(f3sidecar, not(feature = "no-f3-sidecar")))]
use go_ffi::*;

use cid::Cid;
use libp2p::PeerId;

use crate::{networks::ChainConfig, utils::misc::env::is_env_set_and_truthy};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct F3Options {
    pub chain_finality: i64,
    pub bootstrap_epoch: i64,
    pub initial_power_table: Cid,
    pub manifest_server: Option<PeerId>,
}

pub fn get_f3_sidecar_params(chain_config: &ChainConfig) -> F3Options {
    let chain_finality = std::env::var("FOREST_F3_FINALITY")
        .ok()
        .and_then(|v| match v.parse::<i64>() {
            Ok(f) if f > 0 => Some(f),
            _ => {
                tracing::warn!(
                    "Invalid FOREST_F3_FINALITY value {v}. A positive integer is expected."
                );
                None
            }
        })
        .inspect(|i| {
            tracing::info!("Using F3 finality {i} set by FOREST_F3_FINALITY");
        })
        .unwrap_or(chain_config.policy.chain_finality);
    // This will be used post-bootstrap to hard-code the initial F3's initial power table CID.
    // Read from an environment variable for now before the hard-coded value is determined.
    let initial_power_table = std::env::var("FOREST_F3_INITIAL_POWER_TABLE")
        .ok()
        .and_then(|i| i.parse().ok())
        .inspect(|i| {
            tracing::info!(
                "Using F3 initial power table cid {i} set by FOREST_F3_INITIAL_POWER_TABLE"
            )
        })
        .unwrap_or(chain_config.f3_initial_power_table);
    let bootstrap_epoch = std::env::var("FOREST_F3_BOOTSTRAP_EPOCH")
        .ok()
        .and_then(|i| i.parse().ok())
        .inspect(|i| {
            tracing::info!("Using F3 bootstrap epoch {i} set by FOREST_F3_BOOTSTRAP_EPOCH")
        })
        .unwrap_or(chain_config.f3_bootstrap_epoch);
    let manifest_server = match std::env::var("FOREST_F3_MANIFEST_SERVER") {
        Ok(v) => {
            if v.is_empty() {
                None
            } else {
                match v.parse() {
                    Ok(i) => Some(i),
                    _ => {
                        tracing::warn!(
                            "Invalid libp2p peer id {v} set by FOREST_F3_MANIFEST_SERVER"
                        );
                        None
                    }
                }
                .inspect(|i| {
                    tracing::info!("Using F3 manifest server {i} set by FOREST_F3_MANIFEST_SERVER")
                })
                .or(chain_config.f3_manifest_server)
            }
        }
        _ => chain_config.f3_manifest_server,
    };

    F3Options {
        chain_finality,
        bootstrap_epoch,
        initial_power_table,
        manifest_server,
    }
}

pub fn run_f3_sidecar_if_enabled(
    chain_config: &ChainConfig,
    _rpc_endpoint: String,
    _jwt: String,
    _f3_rpc_endpoint: String,
    _initial_power_table: String,
    _bootstrap_epoch: i64,
    _finality: i64,
    _f3_root: String,
    _manifest_server: String,
) {
    if is_sidecar_ffi_enabled(chain_config) {
        #[cfg(all(f3sidecar, not(feature = "no-f3-sidecar")))]
        {
            tracing::info!("Starting F3 sidecar service ...");
            GoF3NodeImpl::run(
                _rpc_endpoint,
                _jwt,
                _f3_rpc_endpoint,
                _initial_power_table,
                _bootstrap_epoch,
                _finality,
                _f3_root,
                _manifest_server,
            );
        }
    }
}

/// Whether F3 sidecar via FFI is enabled.
fn is_sidecar_ffi_enabled(chain_config: &ChainConfig) -> bool {
    // Respect the environment variable when set, and fallback to chain config when not set.
    let enabled =
        is_env_set_and_truthy("FOREST_F3_SIDECAR_FFI_ENABLED").unwrap_or(chain_config.f3_enabled);
    cfg_if::cfg_if! {
        if #[cfg(all(f3sidecar, not(feature = "no-f3-sidecar")))] {
            enabled
        }
        else {
            if enabled {
                tracing::error!("Failed to enable F3 sidecar, the Forest binary is not compiled with f3-sidecar Go lib");
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
            F3Options {
                chain_finality: chain_config.policy.chain_finality,
                bootstrap_epoch: chain_config.f3_bootstrap_epoch,
                initial_power_table: chain_config.f3_initial_power_table,
                manifest_server: chain_config.f3_manifest_server,
            }
        );

        std::env::set_var("FOREST_F3_FINALITY", "100");
        // A random CID
        std::env::set_var(
            "FOREST_F3_INITIAL_POWER_TABLE",
            "bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i",
        );
        std::env::set_var("FOREST_F3_BOOTSTRAP_EPOCH", "100");
        // mainnet server
        std::env::set_var(
            "FOREST_F3_MANIFEST_SERVER",
            "12D3KooWENMwUF9YxvQxar7uBWJtZkA6amvK4xWmKXfSiHUo2Qq7",
        );
        assert_eq!(
            get_f3_sidecar_params(&chain_config),
            F3Options {
                chain_finality: 100,
                bootstrap_epoch: 100,
                initial_power_table: "bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i"
                    .parse()
                    .unwrap(),
                manifest_server: Some(
                    "12D3KooWENMwUF9YxvQxar7uBWJtZkA6amvK4xWmKXfSiHUo2Qq7"
                        .parse()
                        .unwrap()
                ),
            }
        );

        // Unset FOREST_F3_MANIFEST_SERVER
        std::env::set_var("FOREST_F3_MANIFEST_SERVER", "");
        assert_eq!(
            get_f3_sidecar_params(&chain_config),
            F3Options {
                chain_finality: 100,
                bootstrap_epoch: 100,
                initial_power_table: "bafyreicmaj5hhoy5mgqvamfhgexxyergw7hdeshizghodwkjg6qmpoco7i"
                    .parse()
                    .unwrap(),
                manifest_server: None,
            }
        );
    }
}
