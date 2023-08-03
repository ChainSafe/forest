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
use anyhow::{Context, Result};
use lazy_static::lazy_static;
use semver::Version;
use std::fs;
use std::io::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;
use tracing::info;

lazy_static! {
    // Collection of all known database version supporting migration
    static ref KNOWN_VERSIONS: Vec<Version> = {
        let versions: Vec<&str> = vec![
            "0.11.1",
            "0.12.0",
            // Add more versions
        ];

        versions
            .iter()
            .filter_map(|v| Version::parse(v).ok())
            .collect()
    };
}

/// Check to verify database migrations
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

    // Open db
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
    // re-compute 100 tipsets only
    state_manager.validate_range((height - 100)..=height)?;
    Ok(())
}

/// Migrate database to latest version
pub async fn migrate_db(config: &Config, db_path: PathBuf) -> anyhow::Result<()> {
    info!("Running database migrations...");
    // Get DBVersion from existing db path
    let current_version = get_db_version(&db_path)?; //

    // Check migration exist for this version
    if KNOWN_VERSIONS.contains(&current_version) {}
    // Run pre-migration checks, which includes:
    // - re-compute 100 tipsets
    migration_check(config, &db_path).await?;

    let mut temp_db_path = db_path;
    // Iterate over all DBVersion's until database is migrated to latest version
    if let Some(starting_index) = KNOWN_VERSIONS
        .iter()
        .position(|version| *version == current_version)
    {
        let versions_to_migrate = &KNOWN_VERSIONS[starting_index..];

        // Iterate over each version
        for version in versions_to_migrate {
            temp_db_path = migrate(&temp_db_path, version)?;
        }
    }
    // Run post-migration checks, which includes:
    // - re-compute 100 tipsets
    migration_check(config, &temp_db_path).await?;

    // Rename db to latest versioned db
    fs::rename(temp_db_path, chain_path(config))?;

    info!("Database Successfully Migrated");
    Ok(())
}

// TODO: Add Steps required for new migration
/// Migrate to an intermediate db version
fn migrate(db_path: &Path, _version: &Version) -> Result<PathBuf> {
    // Create a temporary directory to store the migrated data.
    let temp_dir = TempDir::new().context("Failed to create a temporary directory")?;

    // TODO: Implement the migration logic here.
    // You should read data from the `_db_path`, perform the necessary
    // migrations based on the specified `_version`, and write the
    // migrated data to the `temp_dir.path()`.

    // If nothing to migrate just rename the existing database
    fs::rename(db_path, temp_dir.path())?;

    // After the migration is complete, you can return the temporary directory path.
    Ok(temp_dir.into_path())
}

/// Checks if another database already exist
pub fn check_if_another_db_exist(config: &Config) -> Option<PathBuf> {
    let dir = PathBuf::from(&config.client.data_dir).join(config.chain.network.to_string());
    let paths = fs::read_dir(&dir).ok()?;
    for dir in paths.flatten() {
        let path = dir.path();
        if path.is_dir() && path != chain_path(config) {
            return Some(path);
        }
    }
    None
}

/// Returns database `Version` for the given database path.
fn get_db_version(db_path: &Path) -> Result<Version, Error> {
    if let Some(name) = db_path.file_name() {
        return name
            .to_str()
            .ok_or_else(|| Error::new(std::io::ErrorKind::InvalidData, "Invalid UTF-8 in path"))
            .and_then(|dir| {
                Version::parse(dir).map_err(|_| {
                    Error::new(std::io::ErrorKind::InvalidData, "Failed to parse version")
                })
            });
    };
    Err(Error::new(
        std::io::ErrorKind::NotFound,
        "File name not found",
    ))
}
