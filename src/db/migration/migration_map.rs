// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, LazyLock},
};

use crate::Config;
use crate::db::migration::v0_22_1::Migration0_22_0_0_22_1;
use crate::db::migration::v0_26_0::Migration0_25_3_0_26_0;
use crate::db::migration::v0_31_0::Migration0_30_5_0_31_0;
use anyhow::Context as _;
use anyhow::bail;
use itertools::Itertools;
use multimap::MultiMap;
use semver::Version;
use tracing::debug;

use super::void_migration::MigrationVoid;

/// Migration trait. It is expected that the [`MigrationOperation::migrate`] method will pick up the relevant database
/// existing under `chain_data_path` and create a new migration database in the same directory.
pub(super) trait MigrationOperation {
    fn new(from: Version, to: Version) -> Self
    where
        Self: Sized;
    /// From version
    fn from(&self) -> &Version;
    /// To version
    fn to(&self) -> &Version;
    /// Performs pre-migration checks. This is the place to check if the database is in a valid
    /// state and if the migration can be performed. Note that some of the higher-level checks
    /// (like checking if the database exists) are performed by the [`Migration`].
    fn pre_checks(&self, chain_data_path: &Path) -> anyhow::Result<()> {
        let old_db = self.old_db_path(chain_data_path);
        anyhow::ensure!(
            old_db.is_dir(),
            "source database {} does not exist",
            old_db.display()
        );
        let new_db = self.new_db_path(chain_data_path);
        anyhow::ensure!(
            !new_db.exists(),
            "target database {} already exists",
            new_db.display()
        );
        let temp_db = self.temporary_db_path(chain_data_path);
        if temp_db.exists() {
            tracing::info!("Removing old temporary database {}", temp_db.display());
            std::fs::remove_dir_all(&temp_db)?;
        }
        Ok(())
    }
    /// Performs the actual migration. All the logic should be implemented here.
    /// Ideally, the migration should use as little of the Forest codebase as possible to avoid
    /// potential issues with the migration code itself and having to update it in the future.
    /// Returns the path to the migrated database (which is not yet validated)
    fn migrate_core(&self, chain_data_path: &Path, config: &Config) -> anyhow::Result<PathBuf>;
    fn migrate(&self, chain_data_path: &Path, config: &Config) -> anyhow::Result<()> {
        self.pre_checks(chain_data_path)?;
        let migrated_db = self.migrate_core(chain_data_path, config)?;
        self.post_checks(chain_data_path)?;

        let new_db = self.new_db_path(chain_data_path);
        debug!(
            "Renaming database {} to {}",
            migrated_db.display(),
            new_db.display()
        );
        std::fs::rename(migrated_db, new_db)?;

        let old_db = self.old_db_path(chain_data_path);
        debug!("Deleting database {}", old_db.display());
        std::fs::remove_dir_all(old_db)?;

        Ok(())
    }
    /// Performs post-migration checks. This is the place to check if the migration database is
    /// ready to be used by Forest and renamed into a versioned database.
    fn post_checks(&self, chain_data_path: &Path) -> anyhow::Result<()> {
        let temp_db_path = self.temporary_db_path(chain_data_path);
        anyhow::ensure!(
            temp_db_path.exists(),
            "temp db {} does not exist",
            temp_db_path.display()
        );
        Ok(())
    }
}

pub trait MigrationOperationExt {
    fn old_db_path(&self, chain_data_path: &Path) -> PathBuf;

    fn new_db_path(&self, chain_data_path: &Path) -> PathBuf;

    fn temporary_db_name(&self) -> String;

    fn temporary_db_path(&self, chain_data_path: &Path) -> PathBuf;
}

impl<T: ?Sized + MigrationOperation> MigrationOperationExt for T {
    fn old_db_path(&self, chain_data_path: &Path) -> PathBuf {
        chain_data_path.join(self.from().to_string())
    }

    fn new_db_path(&self, chain_data_path: &Path) -> PathBuf {
        chain_data_path.join(self.to().to_string())
    }

    fn temporary_db_name(&self) -> String {
        format!("migration_{}_{}", self.from(), self.to()).replace('.', "_")
    }

    fn temporary_db_path(&self, chain_data_path: &Path) -> PathBuf {
        chain_data_path.join(self.temporary_db_name())
    }
}

/// Migrations map. The key is the starting version and the value is the tuple of the target version
/// and the [`MigrationOperation`] implementation.
///
/// In the future we might want to drop legacy migrations (e.g., to clean-up the database
/// dependency that may get several breaking changes).
// If need be, we should introduce "jump" migrations here, e.g. 0.12.0 -> 0.12.2, 0.12.2 -> 0.12.3, etc.
// This would allow us to skip migrations in case of bugs or just for performance reasons.
type Migrator = Arc<dyn MigrationOperation + Send + Sync>;
type MigrationsMap = MultiMap<Version, (Version, Migrator)>;

/// A utility macro to make the migrations easier to declare.
/// The usage is:
/// `<FROM version> -> <TO version> @ <Migrator object>`
macro_rules! create_migrations {
    ($($from:literal -> $to:literal @ $migration:tt),* $(,)?) => {
pub(super) static MIGRATIONS: LazyLock<MigrationsMap> = LazyLock::new(|| {
    MigrationsMap::from_iter(
        [
            $((
            Version::from_str($from).unwrap(),
            (
                Version::from_str($to).unwrap(),
                Arc::new($migration::new(
                        $from.parse().expect("invalid <from> version"),
                        $to.parse().expect("invalid <to> version")))
                as _,
            )),
            )*
        ]
        .iter()
        .cloned(),
    )
});
}}

create_migrations!(
    "0.22.0" -> "0.22.1" @ Migration0_22_0_0_22_1,
    "0.25.3" -> "0.26.0" @ Migration0_25_3_0_26_0,
    "0.30.5" -> "0.31.0" @ Migration0_30_5_0_31_0,
);

/// Creates a migration chain from `start` to `goal`. The chain is chosen to be the shortest
/// possible. If there are multiple shortest paths, any of them is chosen. This method will use
/// the pre-defined migrations map.
pub(super) fn create_migration_chain(
    start: &Version,
    goal: &Version,
) -> anyhow::Result<Vec<Arc<dyn MigrationOperation + Send + Sync>>> {
    create_migration_chain_from_migrations(start, goal, &MIGRATIONS, |from, to| {
        Arc::new(MigrationVoid::new(from.clone(), to.clone()))
    })
}

/// Same as [`create_migration_chain`], but uses any provided migrations map.
fn create_migration_chain_from_migrations(
    start: &Version,
    goal: &Version,
    migrations_map: &MigrationsMap,
    void_migration: impl Fn(&Version, &Version) -> Arc<dyn MigrationOperation + Send + Sync>,
) -> anyhow::Result<Vec<Arc<dyn MigrationOperation + Send + Sync>>> {
    let sorted_from_versions = migrations_map.keys().sorted().collect_vec();
    let result = pathfinding::directed::bfs::bfs(
        start,
        |from| {
            if let Some(migrations) = migrations_map.get_vec(from) {
                migrations.iter().map(|(to, _)| to).cloned().collect()
            } else if let Some(&next) =
                sorted_from_versions.get(sorted_from_versions.partition_point(|&i| i <= from))
            {
                // Jump straight to the next smallest from version in the migration map
                vec![next.clone()]
            } else if goal > from {
                // Or to the goal
                vec![goal.clone()]
            } else {
                // Or fail for downgrading
                vec![]
            }
        },
        |to| to == goal,
    )
    .with_context(|| format!("No migration path found from version {start} to {goal}"))?
    .iter()
    .tuple_windows()
    .map(|(from, to)| {
        migrations_map
            .get_vec(from)
            .map(|v| {
                v.iter()
                    .find_map(|(version, migration)| {
                        if version == to {
                            Some(migration.clone())
                        } else {
                            None
                        }
                    })
                    .expect("Migration must exist")
            })
            .unwrap_or_else(|| void_migration(from, to))
    })
    .collect_vec();

    if result.is_empty() {
        bail!(
            "No migration path found from version {start} to {goal}",
            start = start,
            goal = goal
        );
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::utils::version::FOREST_VERSION;

    #[test]
    fn test_possible_to_migrate_to_current_version() {
        // This test ensures that it is possible to migrate from the oldest supported version to the current
        // version.
        let earliest_version = MIGRATIONS
            .iter_all()
            .map(|(from, _)| from)
            .min()
            .expect("At least one migration must exist");
        let current_version = &FOREST_VERSION;

        let migrations = create_migration_chain(earliest_version, current_version).unwrap();
        assert!(!migrations.is_empty());
    }

    #[test]
    fn test_ensure_migration_possible_from_anywhere_to_latest() {
        // This test ensures that it is possible to find migration chain from any version to the
        // current version.
        let current_version = &FOREST_VERSION;

        for (from, _) in MIGRATIONS.iter_all() {
            let migrations = create_migration_chain(from, current_version).unwrap();
            assert!(!migrations.is_empty());
        }
    }

    #[test]
    fn test_ensure_migration_not_possible_if_higher_than_latest() {
        // This test ensures that it is not possible to migrate from a version higher than the
        // current version.
        let current_version = &FOREST_VERSION;

        let higher_version = Version::new(
            current_version.major,
            current_version.minor,
            current_version.patch + 1,
        );
        let migrations = create_migration_chain(&higher_version, current_version);
        assert!(migrations.is_err());
    }

    #[test]
    fn test_migration_down_not_possible() {
        // This test ensures that it is not possible to migrate down from the latest version.
        // This is not a strict requirement and we may want to allow this in the future.
        let current_version = &*FOREST_VERSION;

        for (from, _) in MIGRATIONS.iter_all() {
            let migrations = create_migration_chain(current_version, from);
            assert!(migrations.is_err());
        }
    }

    #[derive(Debug, Clone)]
    struct EmptyMigration {
        from: Version,
        to: Version,
    }

    impl MigrationOperation for EmptyMigration {
        fn pre_checks(&self, _chain_data_path: &Path) -> anyhow::Result<()> {
            Ok(())
        }

        fn migrate_core(
            &self,
            _chain_data_path: &Path,
            _config: &Config,
        ) -> anyhow::Result<PathBuf> {
            Ok("".into())
        }

        fn post_checks(&self, _chain_data_path: &Path) -> anyhow::Result<()> {
            Ok(())
        }

        fn new(from: Version, to: Version) -> Self
        where
            Self: Sized,
        {
            Self { from, to }
        }

        fn from(&self) -> &Version {
            &self.from
        }

        fn to(&self) -> &Version {
            &self.to
        }
    }

    fn map_empty_migration(
        (from, to): (Version, Version),
    ) -> (
        Version,
        (Version, Arc<dyn MigrationOperation + Send + Sync>),
    ) {
        (
            from.clone(),
            (to.clone(), Arc::new(EmptyMigration::new(from, to)) as _),
        )
    }

    #[test]
    fn test_migration_should_use_shortest_path() {
        let migrations = MigrationsMap::from_iter(
            [
                (Version::new(0, 1, 0), Version::new(0, 2, 0)),
                (Version::new(0, 2, 0), Version::new(0, 3, 0)),
                (Version::new(0, 1, 0), Version::new(0, 3, 0)),
            ]
            .into_iter()
            .map(map_empty_migration),
        );

        let migrations = create_migration_chain_from_migrations(
            &Version::new(0, 1, 0),
            &Version::new(0, 3, 0),
            &migrations,
            |_, _| unimplemented!("void migration"),
        )
        .unwrap();

        // The shortest path is 0.1.0 to 0.3.0 (without going through 0.2.0)
        assert_eq!(1, migrations.len());
        assert_eq!(&Version::new(0, 1, 0), migrations[0].from());
        assert_eq!(&Version::new(0, 3, 0), migrations[0].to());
    }

    #[test]
    fn test_migration_complex_path() {
        let migrations = MigrationsMap::from_iter(
            [
                (Version::new(0, 1, 0), Version::new(0, 2, 0)),
                (Version::new(0, 2, 0), Version::new(0, 3, 0)),
                (Version::new(0, 1, 0), Version::new(0, 3, 0)),
                (Version::new(0, 3, 0), Version::new(0, 3, 1)),
            ]
            .into_iter()
            .map(map_empty_migration),
        );

        let migrations = create_migration_chain_from_migrations(
            &Version::new(0, 1, 0),
            &Version::new(0, 3, 1),
            &migrations,
            |_, _| unimplemented!("void migration"),
        )
        .unwrap();

        // The shortest path is 0.1.0 -> 0.3.0 -> 0.3.1
        assert_eq!(2, migrations.len());
        assert_eq!(&Version::new(0, 1, 0), migrations[0].from());
        assert_eq!(&Version::new(0, 3, 0), migrations[0].to());
        assert_eq!(&Version::new(0, 3, 0), migrations[1].from());
        assert_eq!(&Version::new(0, 3, 1), migrations[1].to());
    }

    #[test]
    fn test_void_migration() {
        let migrations = MigrationsMap::from_iter(
            [
                (Version::new(0, 12, 1), Version::new(0, 13, 0)),
                (Version::new(0, 15, 2), Version::new(0, 16, 0)),
            ]
            .into_iter()
            .map(map_empty_migration),
        );

        let start = Version::new(0, 12, 0);
        let goal = Version::new(1, 0, 0);
        let migrations =
            create_migration_chain_from_migrations(&start, &goal, &migrations, |from, to| {
                Arc::new(EmptyMigration::new(from.clone(), to.clone()))
            })
            .unwrap();

        // The shortest path is 0.12.0 -> 0.12.1 -> 0.13.0 -> 0.15.2 -> 0.16.0 -> 1.0.0
        assert_eq!(5, migrations.len());
        for (a, b) in migrations.iter().zip(migrations.iter().skip(1)) {
            assert_eq!(a.to(), b.from());
        }
        assert_eq!(&start, migrations[0].from());
        assert_eq!(&Version::new(0, 12, 1), migrations[1].from());
        assert_eq!(&Version::new(0, 13, 0), migrations[2].from());
        assert_eq!(&Version::new(0, 15, 2), migrations[3].from());
        assert_eq!(&Version::new(0, 16, 0), migrations[4].from());
        assert_eq!(&goal, migrations[4].to());
    }

    #[test]
    fn test_same_distance_paths_should_yield_any() {
        let migrations = MigrationsMap::from_iter(
            [
                (Version::new(0, 1, 0), Version::new(0, 2, 0)),
                (Version::new(0, 2, 0), Version::new(0, 4, 0)),
                (Version::new(0, 1, 0), Version::new(0, 3, 0)),
                (Version::new(0, 3, 0), Version::new(0, 4, 0)),
            ]
            .into_iter()
            .map(map_empty_migration),
        );

        let migrations = create_migration_chain_from_migrations(
            &Version::new(0, 1, 0),
            &Version::new(0, 4, 0),
            &migrations,
            |_, _| unimplemented!("void migration"),
        )
        .unwrap();

        // there are two possible shortest paths:
        // 0.1.0 -> 0.2.0 -> 0.4.0
        // 0.1.0 -> 0.3.0 -> 0.4.0
        // Both of them are correct and should be accepted.
        assert_eq!(2, migrations.len());
        if migrations[0].to() == &Version::new(0, 2, 0) {
            assert_eq!(&Version::new(0, 1, 0), migrations[0].from());
            assert_eq!(&Version::new(0, 2, 0), migrations[0].to());
            assert_eq!(&Version::new(0, 2, 0), migrations[1].from());
            assert_eq!(&Version::new(0, 4, 0), migrations[1].to());
        } else {
            assert_eq!(&Version::new(0, 1, 0), migrations[0].from());
            assert_eq!(&Version::new(0, 3, 0), migrations[0].to());
            assert_eq!(&Version::new(0, 3, 0), migrations[1].from());
            assert_eq!(&Version::new(0, 4, 0), migrations[1].to());
        }
    }

    struct SimpleMigration0_1_0_0_2_0 {
        from: Version,
        to: Version,
    }

    impl MigrationOperation for SimpleMigration0_1_0_0_2_0 {
        fn migrate_core(
            &self,
            chain_data_path: &Path,
            _config: &Config,
        ) -> anyhow::Result<PathBuf> {
            let temp_db_path = self.temporary_db_path(chain_data_path);
            fs::create_dir(&temp_db_path).unwrap();
            Ok(temp_db_path)
        }

        fn new(from: Version, to: Version) -> Self
        where
            Self: Sized,
        {
            Self { from, to }
        }

        fn from(&self) -> &Version {
            &self.from
        }

        fn to(&self) -> &Version {
            &self.to
        }
    }

    #[test]
    fn test_migration_map_migration() {
        let from = Version::new(0, 1, 0);
        let to = Version::new(0, 2, 0);
        let migration = Arc::new(SimpleMigration0_1_0_0_2_0::new(from, to));

        let temp_dir = TempDir::new().unwrap();

        assert!(migration.pre_checks(temp_dir.path()).is_err());
        fs::create_dir(temp_dir.path().join("0.1.0")).unwrap();
        assert!(migration.pre_checks(temp_dir.path()).is_ok());

        migration
            .migrate(temp_dir.path(), &Config::default())
            .unwrap();
        assert!(temp_dir.path().join("0.2.0").exists());

        assert!(migration.post_checks(temp_dir.path()).is_err());
        fs::create_dir(temp_dir.path().join("migration_0_1_0_0_2_0")).unwrap();
        assert!(migration.post_checks(temp_dir.path()).is_ok());
    }
}
