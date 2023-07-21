// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::db_engine::db_root;
use crate::cli_shared::{chain_path, cli::Config};
use crate::fil_cns::composition as cns;
use crate::state_manager::StateManager;
use crate::utils::proofs_api::paramfetch::{
    ensure_params_downloaded, set_proofs_parameter_cache_dir_env,
};
use fvm_ipld_blockstore::Blockstore;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info};

// migration error types
pub enum MigrationError {
    E1,
    E2,
}

#[derive(Debug, Eq, PartialEq)]
pub enum DBVersion {
    V0,
    V11,
}

/// Check current db
async fn pre_migration_check(config: &Config) -> anyhow::Result<()> {
    let dir = get_existing_db_root(config)?;
    info!("Running database migration checks for: {}", dir.display());

    if cns::FETCH_PARAMS {
        set_proofs_parameter_cache_dir_env(&config.client.data_dir);
    }
    ensure_params_downloaded().await?;

    Ok(())
}

fn get_existing_db_root(config: &Config) -> anyhow::Result<PathBuf> {
    let path = chain_path(config);

    for entry in fs::read_dir(&path)? {
        if let Ok(entry) = entry {
            if entry.file_type()?.is_dir() {
                return Ok(entry.path());
            }
        }
    }

    anyhow::bail!("No Database found.")
}

/// Check new db
async fn post_migration_check<DB>(
    config: &Config,
    state_manager: Arc<StateManager<DB>>,
) -> anyhow::Result<()>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
{
    let dir = db_root(&chain_path(config));
    info!("Running database migration checks for: {}", dir.display());

    if cns::FETCH_PARAMS {
        set_proofs_parameter_cache_dir_env(&config.client.data_dir);
    }
    ensure_params_downloaded().await?;

    let ts = state_manager.chain_store().heaviest_tipset();
    let height = ts.epoch();
    let validate_from = height - 100;
    state_manager.validate_range(validate_from..=height)?;

    Ok(())
}

/// Migrate to an targeted version
pub async fn migrate_db<DB>(
    config: &Config,
    state_manager: Arc<StateManager<DB>>,
    current_version: DBVersion,
    target_version: DBVersion,
) -> anyhow::Result<()>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
{
    info!("Running Database Migrations...");
    pre_migration_check(config).await?;
    // init intermediate with current version
    let mut intermediate_version = current_version;
    while intermediate_version != target_version {
        // Execute the migration steps for itermediate version
        migrate(&intermediate_version)?;
        post_migration_check(config, Arc::clone(&state_manager)).await?;
        // Update the itermediate version
        intermediate_version = match intermediate_version {
            DBVersion::V0 => DBVersion::V11,
            _ => todo!(),
        };
    }
    info!("Database Migrated to {:?}", intermediate_version);
    Ok(())
}

/// Migrate to an intermediate db version
fn migrate(intermediate_version: &DBVersion) -> anyhow::Result<()> {
    match intermediate_version {
        DBVersion::V11 => {
            // Steps required for migrating to V11
            Ok(())
        }
        _ => {
            // Error handling
            Ok(())
        }
    }
}
