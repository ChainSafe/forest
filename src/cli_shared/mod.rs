// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod cli;
pub mod logger;

use std::path::PathBuf;

#[cfg(feature = "mimalloc")]
pub use mimalloc;
#[cfg(feature = "jemalloc")]
pub use tikv_jemallocator;

use crate::networks::NetworkChain;

/// Gets chain data directory
pub fn chain_path(network: &NetworkChain, config: &crate::cli_shared::cli::Config) -> PathBuf {
    PathBuf::from(&config.client.data_dir).join(network.to_string())
}

pub mod snapshot;
