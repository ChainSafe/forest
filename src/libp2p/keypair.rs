// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use tracing::{debug, info, trace};

use crate::{libp2p::Keypair, utils::io::write_new_sensitive_file};
use std::{fs, path::Path};

const KEYPAIR_FILE: &str = "keypair";

/// Returns the libp2p key-pair for the node, generating a new one if it doesn't exist
/// in the data directory.
pub fn get_or_create_keypair(data_dir: &Path) -> anyhow::Result<Keypair> {
    match get_keypair(&data_dir.join(KEYPAIR_FILE)) {
        Some(keypair) => Ok(keypair),
        None => create_and_save_keypair(data_dir),
    }
}

/// Creates and saves a new `ED25519` key-pair to the given path.
/// If an older key-pair exists, it will be backed-up.
/// Returns the generated key-pair.
fn create_and_save_keypair(path: &Path) -> anyhow::Result<Keypair> {
    let gen_keypair = crate::libp2p::ed25519::Keypair::generate();

    let keypair_path = path.join(KEYPAIR_FILE);
    if keypair_path.exists() {
        let mut backup_path = keypair_path.clone();
        backup_path.set_extension("bak");

        info!("Backing up existing keypair to {}", backup_path.display());
        fs::rename(&keypair_path, &backup_path)?;
    }

    write_new_sensitive_file(&gen_keypair.to_bytes(), &keypair_path)?;

    Ok(gen_keypair.into())
}

// Fetch key-pair from disk, returning none if it cannot be decoded.
pub fn get_keypair(path_to_file: &Path) -> Option<Keypair> {
    match std::fs::read(path_to_file) {
        Err(e) => {
            info!("Networking keystore not found!");
            trace!("Error {e}");
            None
        }
        Ok(mut vec) => match crate::libp2p::ed25519::Keypair::try_from_bytes(&mut vec) {
            Ok(kp) => {
                debug!("Recovered libp2p keypair from {}", path_to_file.display());
                Some(kp.into())
            }
            Err(e) => {
                info!("Could not decode networking keystore!");
                info!("Error {e}");
                None
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::remove_file;
    use tempfile::tempdir;

    #[test]
    fn test_get_or_create_keypair() {
        let dir = tempdir().unwrap();
        let path_to_file = dir.path().join(KEYPAIR_FILE);

        // Test that a keypair is generated and saved to disk
        let keypair = get_or_create_keypair(dir.path()).unwrap();
        assert!(path_to_file.exists());

        // Test that the same keypair is returned if it already exists
        let keypair2 = get_or_create_keypair(dir.path()).unwrap();
        assert_eq!(keypair.public(), keypair2.public());
        assert_eq!(
            keypair.clone().try_into_ed25519().unwrap().to_bytes(),
            keypair2.try_into_ed25519().unwrap().to_bytes()
        );

        // Test that a new keypair is generated if the file is deleted
        remove_file(&path_to_file).unwrap();
        let keypair3 = get_or_create_keypair(dir.path()).unwrap();
        assert_ne!(keypair.public(), keypair3.public());
        assert_ne!(
            keypair.try_into_ed25519().unwrap().to_bytes(),
            keypair3.try_into_ed25519().unwrap().to_bytes()
        );
    }

    #[test]
    fn test_backup_keypair() {
        let dir = tempdir().unwrap();
        let path_to_file = dir.path().join(KEYPAIR_FILE);

        // Test that a keypair is generated and saved to disk
        let keypair = create_and_save_keypair(dir.path()).unwrap();
        assert!(path_to_file.exists());

        // corrupt the existing keypair file
        fs::write(
            &path_to_file,
            b"Ph'nglui mglw'nafh Cthulhu R'lyeh wgah'nagl fhtagn",
        )
        .unwrap();

        // Test that a new keypair is generated if the file is corrupted
        // and the old file is backed up
        let keypair2 = get_or_create_keypair(dir.path()).unwrap();
        assert_ne!(keypair.public(), keypair2.public());
        assert_ne!(
            keypair.try_into_ed25519().unwrap().to_bytes(),
            keypair2.try_into_ed25519().unwrap().to_bytes()
        );
        assert!(path_to_file.exists());
        assert!(dir.path().join(format!("{}.bak", KEYPAIR_FILE)).exists());
    }
}
