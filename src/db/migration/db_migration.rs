// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use semver::Version;
use std::path::PathBuf;

use tracing::info;

use crate::{
    db::{
        db_mode::{get_latest_versioned_database, DbMode},
        migration::migration_map::create_migration_chain,
    },
    utils::version::FOREST_VERSION,
};

/// Governs the database migration process. This is the entry point for the migration process.
pub struct DbMigration {
    /// Root of the chain data directory. This is where all the databases are stored.
    chain_data_path: PathBuf,
}

impl DbMigration {
    pub fn new(chain_data_path: PathBuf) -> Self {
        Self { chain_data_path }
    }

    /// Verifies if a migration is needed for the current Forest process.
    /// Note that migration can possibly happen only if the DB is in `current` mode.
    pub fn is_migration_required(&self) -> anyhow::Result<bool> {
        // No chain data means that this is a fresh instance. No migration required.
        if !self.chain_data_path.exists() {
            return Ok(false);
        }

        // migration is required only if the DB is in `current` mode and the current db version is
        // smaller than the current binary version
        if let DbMode::Current = DbMode::read() {
            let current_db = get_latest_versioned_database(&self.chain_data_path)?
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

        let latest_db_version = get_latest_versioned_database(&self.chain_data_path)?
            .unwrap_or_else(|| FOREST_VERSION.clone());

        info!(
            "Migrating database from version {} to {}",
            latest_db_version, *FOREST_VERSION
        );

        let target_db_version = &FOREST_VERSION;

        let migrations = create_migration_chain(&latest_db_version, target_db_version)?;

        for migration in migrations {
            migration.migrate(&self.chain_data_path)?;
        }

        info!(
            "Migration to version {} complete",
            target_db_version.to_string()
        );

        Ok(())
    }
}

pub(crate) fn db_name(from: &Version, to: &Version) -> String {
    format!("migration_{}_{}", from, to).replace('.', "_")
}

#[cfg(test)]
mod tests {
    use crate::db::db_mode::FOREST_DB_DEV_MODE;

    use super::*;

    #[test]
    fn test_migration_not_required_no_chain_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_migration = DbMigration::new(temp_dir.path().join("azathoth"));
        assert!(!db_migration.is_migration_required().unwrap());
    }

    #[test]
    fn test_migration_not_required_no_databases() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_migration = DbMigration::new(temp_dir.path().to_owned());
        assert!(!db_migration.is_migration_required().unwrap());
    }

    #[test]
    fn test_migration_not_required_under_non_current_mode() {
        let temp_dir = tempfile::tempdir().unwrap();

        let db_dir = temp_dir.path().join("0.1.0");
        std::fs::create_dir(&db_dir).unwrap();
        let db_migration = DbMigration::new(temp_dir.path().to_owned());

        std::env::set_var(FOREST_DB_DEV_MODE, "latest");
        assert!(!db_migration.is_migration_required().unwrap());

        std::fs::remove_dir(db_dir).unwrap();
        std::fs::create_dir(temp_dir.path().join("cthulhu")).unwrap();

        std::env::set_var(FOREST_DB_DEV_MODE, "cthulhu");
        assert!(!db_migration.is_migration_required().unwrap());
    }

    #[test]
    fn test_migration_required_current_mode() {
        let temp_dir = tempfile::tempdir().unwrap();

        let db_dir = temp_dir.path().join("0.1.0");
        std::fs::create_dir(db_dir).unwrap();
        let db_migration = DbMigration::new(temp_dir.path().to_owned());

        std::env::set_var(FOREST_DB_DEV_MODE, "current");
        assert!(db_migration.is_migration_required().unwrap());
        std::env::remove_var(FOREST_DB_DEV_MODE);
        assert!(db_migration.is_migration_required().unwrap());
    }
}
