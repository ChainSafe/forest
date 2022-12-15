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

#[cfg(feature = "rocksdb")]
/// Gets database directory
pub fn db_path(config: &crate::cli::Config) -> PathBuf {
    chain_path(config).join("rocksdb")
}

#[cfg(feature = "paritydb")]
/// Gets database directory
pub fn db_path(config: &crate::cli::Config) -> PathBuf {
    chain_path(config).join("paritydb")
}

pub fn open_db(path: &std::path::Path, config: &cli::Config) -> Result<Db, forest_db::Error> {
    #[cfg(feature = "rocksdb")]
    {
        forest_db::rocks::RocksDb::open(path, &config.db)
    }
    #[cfg(feature = "paritydb")]
    {
        let paritydb_config = forest_db::parity_db::ParityDbConfig {
            columns: config.db.columns,
        };
        forest_db::parity_db::ParityDb::open(path, &paritydb_config)
    }
}
