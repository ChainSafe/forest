// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Migration logic from any version that requires no migration logic.

use fs_extra::dir::CopyOptions;
use semver::Version;
use std::path::{Path, PathBuf};
use tracing::info;

use super::migration_map::MigrationOperation;

pub(super) struct MigrationVoid {
    from: Version,
    to: Version,
}

impl MigrationOperation for MigrationVoid {
    fn pre_checks(&self, _chain_data_path: &Path) -> anyhow::Result<()> {
        Ok(())
    }

    fn migrate(&self, chain_data_path: &Path) -> anyhow::Result<PathBuf> {
        let source_db = chain_data_path.join(self.from.to_string());

        let temp_db_path = chain_data_path.join(self.temporary_db_name());
        if temp_db_path.exists() {
            info!(
                "removing old temporary database {temp_db_path}",
                temp_db_path = temp_db_path.display()
            );
            std::fs::remove_dir_all(&temp_db_path)?;
        }

        info!(
            "copying old database from {source_db} to {temp_db_path}",
            source_db = source_db.display(),
            temp_db_path = temp_db_path.display()
        );
        fs_extra::copy_items(
            &[source_db.as_path()],
            temp_db_path.clone(),
            &CopyOptions::default().copy_inside(true),
        )?;

        Ok(temp_db_path)
    }

    fn post_checks(&self, chain_data_path: &Path) -> anyhow::Result<()> {
        let temp_db_name = self.temporary_db_name();
        if !chain_data_path.join(&temp_db_name).exists() {
            anyhow::bail!(
                "migration database {} does not exist",
                chain_data_path.join(temp_db_name).display()
            );
        }
        Ok(())
    }

    fn new(from: Version, to: Version) -> Self
    where
        Self: Sized,
    {
        Self { from, to }
    }

    fn temporary_db_name(&self) -> String {
        format!("migration_{}_{}", self.from, self.to).replace('.', "_")
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
        migration.pre_checks(path).unwrap();
        let temp_db_path = migration.migrate(path).unwrap();
        migration.post_checks(path).unwrap();

        // check that the temporary database directory exists and contains the file with the
        // expected content.
        let temp_db_content_dir = temp_db_path.join("R'lyeh");
        let temp_db_content_file = temp_db_content_dir.join("cthulhu");
        assert!(temp_db_content_file.exists());
        assert_eq!(
            std::fs::read_to_string(temp_db_content_file).unwrap(),
            chant
        );
    }
}
