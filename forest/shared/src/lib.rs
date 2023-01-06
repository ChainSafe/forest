// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod cli;
pub mod logger;

use std::path::{Path, PathBuf};

#[cfg(feature = "rocksdb")]
pub type Db = forest_db::rocks::RocksDb;

#[cfg(feature = "paritydb")]
pub type Db = forest_db::parity_db::ParityDb;

#[cfg(feature = "rocksdb")]
pub type DbConfig = forest_db::rocks_config::RocksDbConfig;

#[cfg(feature = "paritydb")]
pub type DbConfig = forest_db::parity_db_config::ParityDbConfig;

/// Gets chain data directory
pub fn chain_path(config: &crate::cli::Config) -> PathBuf {
    PathBuf::from(&config.client.data_dir).join(&config.chain.name)
}

pub fn db_path(path: &Path) -> PathBuf {
    #[cfg(feature = "rocksdb")]
    {
        path.join("rocksdb")
    }
    #[cfg(feature = "paritydb")]
    {
        path.join("paritydb")
    }
}

pub fn open_db(path: &std::path::Path, config: &DbConfig) -> anyhow::Result<Db> {
    #[cfg(feature = "rocksdb")]
    {
        forest_db::rocks::RocksDb::open(path, config).map_err(Into::into)
    }
    #[cfg(feature = "paritydb")]
    {
        use forest_db::parity_db::*;
        ParityDb::open(path.to_owned(), config)
    }
}
