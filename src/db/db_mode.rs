// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use semver::Version;

use crate::utils::version::FOREST_VERSION;

/// Environment variable used to set the development mode
/// It is used for development purposes. Possible values:
/// - `current`: use the database matching the current binary version. May result in migrations.
/// - `latest`: use the latest database version.
/// - other values: use the database matching the provided name.
pub(super) const FOREST_DB_DEV_MODE: &str = "FOREST_DB_DEV_MODE";

/// Lists all versioned databases in the chain data directory.
/// Versioned databases are directories with a `SemVer` version as their name. The rest is discarded.
fn list_versioned_databases(chain_data_path: &Path) -> anyhow::Result<Vec<Version>> {
    let versions = fs::read_dir(chain_data_path)?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let version = Version::parse(path.file_name()?.to_str()?);
            match version {
                Ok(version) => Some(version),
                // Ignore any directories that are not valid semver versions. Those might be
                // development databases.
                Err(_) => None,
            }
        })
        .collect();

    Ok(versions)
}

/// Returns the latest versioned database in the chain data directory (if such one exists).
pub(super) fn get_latest_versioned_database(
    chain_data_path: &Path,
) -> anyhow::Result<Option<Version>> {
    let versions = list_versioned_databases(chain_data_path)?;
    Ok(versions.iter().max().cloned())
}

/// Chooses the correct database directory to use based on the `[FOREST_DB_DEV_MODE]`
/// environment variable (or the lack of it).
pub fn choose_db(chain_data_path: &Path) -> anyhow::Result<PathBuf> {
    let db = match DbMode::read() {
        DbMode::Current => chain_data_path.join(FOREST_VERSION.to_string()),
        DbMode::Latest => {
            let versions = list_versioned_databases(chain_data_path)?;

            if versions.is_empty() {
                chain_data_path.join(FOREST_VERSION.to_string())
            } else {
                let latest = versions
                    .iter()
                    .max()
                    .context("Failed to find latest versioned database")?; // This should never happen
                chain_data_path.join(latest.to_string())
            }
        }
        DbMode::Custom(custom) => chain_data_path.join(custom),
    };

    Ok(db)
}

/// Represents different modes of access to the database
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum DbMode {
    /// Using the database matching the binary version. This is the default, and is the only mode
    /// in which migrations are run.
    Current,
    /// Using the latest versioned database if exists
    Latest,
    /// Using a custom database
    Custom(String),
}

impl DbMode {
    /// Returns the database mode based on the environment variable
    pub fn read() -> Self {
        match std::env::var(FOREST_DB_DEV_MODE)
            .map(|s| s.to_lowercase())
            .as_deref()
        {
            Ok("latest") => Self::Latest,
            Ok("current") | Err(_) => Self::Current,
            Ok(val) => Self::Custom(val.to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use super::*;
    use std::env;

    #[test]
    fn test_db_mode() {
        env::set_var(FOREST_DB_DEV_MODE, "latest");
        assert_eq!(DbMode::read(), DbMode::Latest);

        env::set_var(FOREST_DB_DEV_MODE, "current");
        assert_eq!(DbMode::read(), DbMode::Current);

        env::set_var(FOREST_DB_DEV_MODE, "cthulhu");
        assert_eq!(DbMode::read(), DbMode::Custom("cthulhu".to_owned()));

        env::remove_var(FOREST_DB_DEV_MODE);
        assert_eq!(DbMode::read(), DbMode::Current);
    }

    #[test]
    fn test_list_versioned_databases() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path();

        for dir in &["0.1.0", "0.2.0", "0.3.0", "Elder God", "my0.4.0"] {
            std::fs::create_dir(path.join(dir)).unwrap();
        }

        let versions = list_versioned_databases(path)
            .unwrap()
            .iter()
            .sorted()
            .cloned()
            .collect_vec();
        assert_eq!(
            versions,
            vec![
                Version::parse("0.1.0").unwrap(),
                Version::parse("0.2.0").unwrap(),
                Version::parse("0.3.0").unwrap()
            ]
        );
    }

    #[test]
    fn test_choose_db() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path();

        for dir in &["0.1.0", "0.2.0", "0.3.0", "Elder God", "my0.4.0"] {
            std::fs::create_dir(path.join(dir)).unwrap();
        }

        let cases = [
            ("latest", path.join("0.3.0")),
            ("current", path.join(FOREST_VERSION.to_string())),
            ("cthulhu", path.join("cthulhu")),
        ];

        for (mode, expected) in &cases {
            env::set_var(FOREST_DB_DEV_MODE, mode);
            let db = choose_db(path).unwrap();
            assert_eq!(db, *expected);
        }

        env::remove_var(FOREST_DB_DEV_MODE);
        let db = choose_db(path).unwrap();
        assert_eq!(db, path.join(FOREST_VERSION.to_string()));
    }
}
