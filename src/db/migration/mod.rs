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
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Eq, PartialEq)]
pub enum DBVersion {
    V0, // Default DBVersion for any unknow db
    V11,
}

/// Check migration
async fn migration_check(
    config: &Config,
    existing_chain_data_root: &PathBuf,
) -> anyhow::Result<()> {
    info!(
        "Running database migration checks for: {}",
        existing_chain_data_root.display()
    );

    if ensure_params_downloaded().await.is_err() || cns::FETCH_PARAMS {
        set_proofs_parameter_cache_dir_env(&config.client.data_dir);
    }
    ensure_params_downloaded().await?;

    let db = Arc::new(open_proxy_db(
        db_root(&existing_chain_data_root),
        config.db_config().clone(),
    )?);
    let genesis = read_genesis_header(None, config.chain.genesis_bytes(), &db).await?;
    let chain_store = Arc::new(ChainStore::new(
        db,
        Arc::clone(&config.chain),
        &genesis,
        existing_chain_data_root.as_path(),
    )?);
    let state_manager = Arc::new(StateManager::new(chain_store, Arc::clone(&config.chain))?);

    let ts = state_manager.chain_store().heaviest_tipset();
    let height = ts.epoch();
    assert!(height.is_positive());
    state_manager.validate_range((height - 1)..=height)?;
    Ok(())
}

/// Migrate to an targeted version
pub async fn migrate_db(
    config: &Config,
    db_path: PathBuf,
    target_version: DBVersion,
) -> anyhow::Result<()> {
    info!("Running database migrations...");

    let mut current_version = get_db_version(&db_path);

    migration_check(config, &db_path).await?;

    while current_version != target_version {
        let next_version = match current_version {
            DBVersion::V0 => DBVersion::V11,
            _ => break,
        };
        // Execute the migration steps for itermediate version
        migrate(&db_path, &next_version)?;
        current_version = next_version;
    }

    migration_check(config, &db_path).await?;

    // Rename db to latest versioned db
    fs::rename(db_path.as_path(), &chain_path(config))?;

    info!("Database Migrated to {:?}", target_version);
    Ok(())
}

/// Migrate to an intermediate db version
fn migrate(_existing_db_path: &PathBuf, intermediate_version: &DBVersion) -> anyhow::Result<()> {
    match intermediate_version {
        DBVersion::V11 => {
            // TODO: Add Steps required for migrating to V11
            Ok(())
        }
        _ => Ok(()),
    }
}

pub fn check_if_another_db_exist(config: &Config) -> Option<PathBuf> {
    let dir = PathBuf::from(&config.client.data_dir).join(config.chain.network.to_string());
    let paths = fs::read_dir(&dir).unwrap();
    for path in paths {
        if let Ok(entry) = path {
            return Some(entry.path());
        }
    }

    None
}

fn get_db_version(db_path: &PathBuf) -> DBVersion {
    match db_path
        .parent()
        .and_then(|parent_path| parent_path.file_name())
    {
        Some(dir_name) => match dir_name.to_str() {
            Some("0.11.1") => DBVersion::V11,
            _ => DBVersion::V0,
        },
        None => DBVersion::V0,
    }
}
