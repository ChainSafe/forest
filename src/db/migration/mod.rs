// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::db_engine::{db_root, open_proxy_db};
use crate::chain::ChainStore;
use crate::cli_shared::{chain_path, cli::Config};
use crate::fil_cns::composition as cns;
use crate::genesis::read_genesis_header;
use crate::state_manager::StateManager;
use crate::utils::proofs_api::paramfetch::{
    ensure_params_downloaded, set_proofs_parameter_cache_dir_env,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

pub const LATEST_DB_VERSION: DBVersion = DBVersion::V11;

// TODO: Add a new enum variant for every new database version
/// Database version for each forest version which supports db migration
#[derive(Debug, Eq, PartialEq)]
pub enum DBVersion {
    V0, // Default DBVersion for any unknown db
    V11,
}

/// Check database validaity
async fn migration_check(config: &Config, existing_chain_data_root: &Path) -> anyhow::Result<()> {
    info!(
        "Running database migration checks for: {}",
        existing_chain_data_root.display()
    );
    // Set proof param dir env path, required for running validations
    if cns::FETCH_PARAMS {
        set_proofs_parameter_cache_dir_env(&config.client.data_dir);
    }
    ensure_params_downloaded().await?;

    // Open existing db
    let db = Arc::new(open_proxy_db(
        db_root(existing_chain_data_root),
        config.db_config().clone(),
    )?);
    let genesis = read_genesis_header(None, config.chain.genesis_bytes(), &db).await?;
    let chain_store = Arc::new(ChainStore::new(
        db,
        Arc::clone(&config.chain),
        genesis,
        existing_chain_data_root,
    )?);
    let state_manager = Arc::new(StateManager::new(chain_store, Arc::clone(&config.chain))?);

    let ts = state_manager.chain_store().heaviest_tipset();
    let height = ts.epoch();
    assert!(height.is_positive());
    // re-compute 100 tipsets only
    state_manager.validate_range((height - 100)..=height)?;
    Ok(())
}

/// Migrate database to lastest version
pub async fn migrate_db(
    config: &Config,
    db_path: PathBuf,
    target_version: DBVersion,
) -> anyhow::Result<()> {
    info!("Running database migrations...");
    // Get DBVersion from existing db path
    let mut current_version = get_db_version(&db_path);
    // Run pre-migration checks, which includes:
    // - re-compute 100 tipsets
    migration_check(config, &db_path).await?;

    // Iterate over all DBVersion's until database is migrated to lastest version
    while current_version != target_version {
        let next_version = match current_version {
            // TODO: Add version transition for each DBVersion
            DBVersion::V0 => DBVersion::V11,
            _ => break,
        };
        // Execute the migration steps for itermediate version
        migrate(&db_path, &next_version)?;
        current_version = next_version;
    }
    // Run post-migration checks, which includes:
    // - re-compute 100 tipsets
    migration_check(config, &db_path).await?;

    // Rename db to latest versioned db
    fs::rename(db_path.as_path(), chain_path(config))?;

    info!("Database Successfully Migrated to {:?}", target_version);
    Ok(())
}

// TODO: Add Steps required for new migration
/// Migrate to an intermediate db version
fn migrate(_existing_db_path: &Path, next_version: &DBVersion) -> anyhow::Result<()> {
    match next_version {
        DBVersion::V11 => Ok(()),
        _ => Ok(()),
    }
}

/// Checks if another db already exist
pub fn check_if_another_db_exist(config: &Config) -> Option<PathBuf> {
    let dir = PathBuf::from(&config.client.data_dir).join(config.chain.network.to_string());
    let paths = fs::read_dir(&dir).unwrap();
    for dir in paths.flatten() {
        let path = dir.path();
        if path.is_dir() && path != chain_path(config) {
            return Some(path);
        }
    }
    None
}

/// Returns respective `DBVersion` from db dir name
fn get_db_version(db_path: &Path) -> DBVersion {
    match db_path
        .parent()
        .and_then(|parent_path| parent_path.file_name())
    {
        Some(dir_name) => match dir_name.to_str() {
            Some(name) if name.starts_with("0.11") => DBVersion::V11,
            _ => DBVersion::V0, // Defaults to V0
        },
        None => DBVersion::V0, // Defaults to V0
    }
}
