// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod cli;
pub mod logger;

use std::path::PathBuf;

/// Gets chain data directory
pub fn chain_path(config: &crate::cli::Config) -> PathBuf {
    PathBuf::from(&config.client.data_dir).join(&config.chain.name)
}
