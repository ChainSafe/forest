// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use tracing::info;

use crate::{
    Config,
    cli_shared::chain_path,
    db::{
        db_mode::{DbMode, get_latest_versioned_database},
        migration::migration_map::create_migration_chain,
    },
    utils::version::FOREST_VERSION,
};

/// Governs the database migration process. This is the entry point for the migration process.
pub struct DbMigration {
    /// Forest configuration used.
    config: Config,
}

impl DbMigration {
    pub fn new(config: &Config) -> Self {
        Self {
            config: config.clone(),
        }
    }

    pub fn chain_data_path(&self) -> PathBuf {
        chain_path(&self.config)
    }

    /// Verifies if a migration is needed for the current Forest process.
    /// Note that migration can possibly happen only if the DB is in `current` mode.
    pub fn is_migration_required(&self) -> anyhow::Result<bool> {
        // No chain data means that this is a fresh instance. No migration required.
        if !self.chain_data_path().exists() {
            return Ok(false);
        }

        // migration is required only if the DB is in `current` mode and the current db version is
        // smaller than the current binary version
        if let DbMode::Current = DbMode::read() {
            let current_db = get_latest_versioned_database(&self.chain_data_path())?
                .unwrap_or_else(|| FOREST_VERSION.clone());
            Ok(current_db < *FOREST_VERSION)
        } else {
            Ok(false)
        }
    }

    /// Performs a database migration if required. Note that this may take a long time to complete
    /// and may need a lot of disk space (at least twice the size of the current database).
    /// On a successful migration, the current database will be removed and the new database will
    /// be used.
    /// This method is tested via integration tests.
    pub fn migrate(&self) -> anyhow::Result<()> {
        if !self.is_migration_required()? {
            info!("No database migration required");
            return Ok(());
        }

        let latest_db_version = get_latest_versioned_database(&self.chain_data_path())?
            .unwrap_or_else(|| FOREST_VERSION.clone());

        let target_db_version = &FOREST_VERSION;

        let migrations = create_migration_chain(&latest_db_version, target_db_version)?;

        for migration in migrations {
            info!(
                "Migrating database from version {} to {}",
                migration.from(),
                migration.to()
            );
            let start = std::time::Instant::now();
            migration.migrate(&self.chain_data_path(), &self.config)?;
            info!(
                "Successfully migrated from version {} to {}, took {}",
                migration.from(),
                migration.to(),
                humantime::format_duration(std::time::Instant::now() - start),
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::db_mode::FOREST_DB_DEV_MODE;

    #[test]
    fn test_migration_not_required_no_chain_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.client.data_dir = temp_dir.path().join("azathoth");
        let db_migration = DbMigration::new(&config);
        assert!(!db_migration.is_migration_required().unwrap());
    }

    #[test]
    fn test_migration_not_required_no_databases() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        temp_dir.path().clone_into(&mut config.client.data_dir);
        let db_migration = DbMigration::new(&config);
        assert!(!db_migration.is_migration_required().unwrap());
    }

    #[test]
    fn test_migration_not_required_under_non_current_mode() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        temp_dir.path().clone_into(&mut config.client.data_dir);

        let db_dir = temp_dir.path().join("mainnet/0.1.0");
        std::fs::create_dir_all(&db_dir).unwrap();
        let db_migration = DbMigration::new(&config);

        unsafe { std::env::set_var(FOREST_DB_DEV_MODE, "latest") };
        assert!(!db_migration.is_migration_required().unwrap());

        std::fs::remove_dir(db_dir).unwrap();
        std::fs::create_dir_all(temp_dir.path().join("mainnet/cthulhu")).unwrap();

        unsafe { std::env::set_var(FOREST_DB_DEV_MODE, "cthulhu") };
        assert!(!db_migration.is_migration_required().unwrap());
    }

    #[test]
    fn test_migration_required_current_mode() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        temp_dir.path().clone_into(&mut config.client.data_dir);

        let db_dir = temp_dir.path().join("mainnet/0.1.0");
        std::fs::create_dir_all(db_dir).unwrap();
        let db_migration = DbMigration::new(&config);

        unsafe { std::env::set_var(FOREST_DB_DEV_MODE, "current") };
        assert!(db_migration.is_migration_required().unwrap());
        unsafe { std::env::remove_var(FOREST_DB_DEV_MODE) };
        assert!(db_migration.is_migration_required().unwrap());
    }
}
