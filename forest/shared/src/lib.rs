// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod cli;
pub mod logger;

use std::path::PathBuf;

#[cfg(feature = "rocksdb")]
pub type Db = forest_db::rocks::RocksDb;

#[cfg(feature = "paritydb")]
pub type Db = forest_db::parity_db::ParityDb;

/// Gets chain data directory
pub fn chain_path(config: &crate::cli::Config) -> PathBuf {
    PathBuf::from(&config.client.data_dir).join(&config.chain.name)
}

pub fn db_path(config: &crate::cli::Config) -> PathBuf {
    #[cfg(feature = "rocksdb")]
    {
        chain_path(config).join("rocksdb")
    }
    #[cfg(feature = "paritydb")]
    {
        chain_path(config).join("paritydb")
    }
}

#[cfg(feature = "rocksdb")]
pub fn open_db(
    path: &std::path::Path,
    config: &cli::Config,
) -> anyhow::Result<forest_db::rocks::RocksDb> {
    forest_db::rocks::RocksDb::open(path, &config.rocks_db).map_err(Into::into)
}

#[cfg(feature = "paritydb")]
pub fn open_db(
    path: &std::path::Path,
    config: &cli::Config,
) -> anyhow::Result<forest_db::parity_db::ParityDb> {
    use forest_db::parity_db::*;
    ParityDb::open(path.to_owned(), &config.parity_db)
}
