// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Migration logic from any version that requires no migration logic.

use super::migration_map::MigrationOperationExt as _;
use crate::Config;
use semver::Version;
use std::path::{Path, PathBuf};

use super::migration_map::MigrationOperation;

pub(super) struct MigrationVoid {
    from: Version,
    to: Version,
}

impl MigrationOperation for MigrationVoid {
    fn migrate_core(&self, _: &Path, _config: &Config) -> anyhow::Result<PathBuf> {
        unimplemented!("Overriding migrate implementation instead")
    }

    fn migrate(&self, chain_data_path: &Path, _: &Config) -> anyhow::Result<()> {
        self.pre_checks(chain_data_path)?;
        let old_db = self.old_db_path(chain_data_path);
        let new_db = self.new_db_path(chain_data_path);
        tracing::debug!(
            "Renaming database {} to {}",
            old_db.display(),
            new_db.display()
        );
        std::fs::rename(old_db, new_db)?;
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

#[cfg(test)]
mod test {
    use super::*;
    use semver::Version;
    use tempfile::TempDir;

    #[test]
    fn test_migration_void() {
        let migration = MigrationVoid::new(Version::new(1, 0, 0), Version::new(1, 0, 1));
        let chain_data_path = TempDir::new().unwrap();

        // create a file in the temporary database directory, under a directory (to ensure that the
        // directory is copied recursively).
        let content_dir = chain_data_path.path().join("1.0.0").join("R'lyeh");
        std::fs::create_dir_all(&content_dir).unwrap();

        let content_file = content_dir.join("cthulhu");
        let chant = "ph'nglui mglw'nafh Cthulhu R'lyeh wgah'nagl fhtagn";
        std::fs::write(content_file, chant).unwrap();

        let path = chain_data_path.path();
        migration.migrate(path, &Config::default()).unwrap();
        let new_db_path = migration.new_db_path(path);

        // check that the target database directory exists and contains the file with the
        // expected content.
        let new_db_content_dir = new_db_path.join("R'lyeh");
        let new_db_content_file = new_db_content_dir.join("cthulhu");
        assert!(new_db_content_file.exists());
        assert_eq!(std::fs::read_to_string(new_db_content_file).unwrap(), chant);
    }
}
