// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use tracing::{info, trace};

use crate::{
    libp2p::Keypair,
    utils::io::{read_file_to_vec, write_to_file},
};
use std::{fs, path::Path};

const KEYPAIR_FILE: &str = "keypair";

/// Returns the libp2p key-pair for the node, generating a new one if it doesn't exist
/// in the data directory.
pub fn get_or_create_keypair(data_dir: &Path) -> anyhow::Result<Keypair> {
    match get_keypair(data_dir) {
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
        fs::rename(keypair_path, &backup_path)?;
    }

    let file = write_to_file(&gen_keypair.to_bytes(), path, KEYPAIR_FILE)?;
    // Restrict permissions on files containing private keys
    crate::utils::io::set_user_perm(&file)?;

    Ok(gen_keypair.into())
}

// Fetch key-pair from disk, returning none if it cannot be decoded.
fn get_keypair(data_dir: &Path) -> Option<Keypair> {
    let path_to_file = data_dir.join(KEYPAIR_FILE);
    match read_file_to_vec(&path_to_file) {
        Err(e) => {
            info!("Networking keystore not found!");
            trace!("Error {e}");
            None
        }
        Ok(mut vec) => match crate::libp2p::ed25519::Keypair::try_from_bytes(&mut vec) {
            Ok(kp) => {
                info!("Recovered libp2p keypair from {}", path_to_file.display());
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
    fn test_get_or_create_keypair() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let path_to_file = dir.path().join(KEYPAIR_FILE);

        // Test that a keypair is generated and saved to disk
        let keypair = get_or_create_keypair(dir.path())?;
        assert!(path_to_file.exists());

        // Test that the same keypair is returned if it already exists
        let keypair2 = get_or_create_keypair(dir.path())?;
        assert_eq!(keypair.public(), keypair2.public());
        assert_eq!(
            keypair.clone().try_into_ed25519()?.to_bytes(),
            keypair2.try_into_ed25519()?.to_bytes()
        );

        // Test that a new keypair is generated if the file is deleted
        remove_file(&path_to_file)?;
        let keypair3 = get_or_create_keypair(dir.path())?;
        assert_ne!(keypair.public(), keypair3.public());
        assert_ne!(
            keypair.try_into_ed25519()?.to_bytes(),
            keypair3.try_into_ed25519()?.to_bytes()
        );

        Ok(())
    }

    #[test]
    fn test_backup_keypair() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let path_to_file = dir.path().join(KEYPAIR_FILE);

        // Test that a keypair is generated and saved to disk
        let keypair = create_and_save_keypair(dir.path())?;
        assert!(path_to_file.exists());

        // corrupt the existing keypair file
        fs::write(
            &path_to_file,
            b"Ph'nglui mglw'nafh Cthulhu R'lyeh wgah'nagl fhtagn",
        )?;

        // Test that a new keypair is generated if the file is corrupted
        // and the old file is backed up
        let keypair2 = get_or_create_keypair(dir.path())?;
        assert_ne!(keypair.public(), keypair2.public());
        assert_ne!(
            keypair.try_into_ed25519()?.to_bytes(),
            keypair2.try_into_ed25519()?.to_bytes()
        );
        assert!(path_to_file.exists());
        assert!(dir.path().join(format!("{}.bak", KEYPAIR_FILE)).exists());
        Ok(())
    }
}
