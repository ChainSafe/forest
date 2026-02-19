// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::bail;
use clap::Subcommand;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

use crate::{cli_shared::read_config, networks::NetworkChain};

#[derive(Subcommand)]
pub enum BackupCommands {
    /// Create a backup of the node. By default, only the peer-to-peer key-pair and key-store are backed up.
    /// The node must be offline.
    Create {
        /// Path to the output backup file if not using the default
        #[arg(long)]
        backup_file: Option<PathBuf>,
        /// Backup everything from the Forest data directory. This will override other options.
        #[arg(long)]
        all: bool,
        /// Disables backing up the key-pair
        #[arg(long)]
        no_keypair: bool,
        /// Disables backing up the key-store
        #[arg(long)]
        no_keystore: bool,
        /// Backs up the blockstore for the specified chain. If not provided, it will not be backed up.
        #[arg(long)]
        backup_chain: Option<NetworkChain>,
        /// Include proof parameters in the backup
        #[arg(long)]
        include_proof_params: bool,
        /// Optional TOML file containing forest daemon configuration. If not provided, the default configuration will be used.
        #[arg(short, long)]
        daemon_config: Option<PathBuf>,
    },
    /// Restore a backup of the node from a file. The node must be offline.
    Restore {
        /// Path to the backup file
        backup_file: PathBuf,
        /// Optional TOML file containing forest daemon configuration. If not provided, the default configuration will be used.
        #[arg(short, long)]
        daemon_config: Option<PathBuf>,
        /// Force restore even if files already exist
        /// WARNING: This will overwrite existing files.
        #[arg(long)]
        force: bool,
    },
}

impl BackupCommands {
    pub fn run(self) -> anyhow::Result<()> {
        match self {
            BackupCommands::Create {
                backup_file,
                all,
                no_keypair,
                no_keystore,
                backup_chain,
                include_proof_params,
                daemon_config,
            } => {
                let (_, config) = read_config(daemon_config.as_ref(), backup_chain.clone())?;

                let data_dir = &config.client.data_dir;

                let backup_entries = if all {
                    std::fs::read_dir(data_dir)?
                        .filter_map(Result::ok)
                        .map(|e| e.path())
                        .collect()
                } else {
                    validate_and_add_entries(
                        data_dir,
                        no_keypair,
                        no_keystore,
                        backup_chain,
                        include_proof_params,
                    )?
                };

                let backup_file_path = if let Some(backup_file) = backup_file {
                    backup_file
                } else {
                    let path = PathBuf::from(format!(
                        "forest-backup-{}.tar",
                        chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S")
                    ));
                    if path.exists() {
                        bail!("Backup file already exists at {}", path.display());
                    }
                    path
                };

                archive_entries(data_dir, backup_entries, &backup_file_path)?;
                println!("Backup complete: {}", backup_file_path.display());

                Ok(())
            }
            BackupCommands::Restore {
                backup_file,
                daemon_config,
                force,
            } => {
                let (_, config) = read_config(daemon_config.as_ref(), None)?;
                let data_dir = &config.client.data_dir;

                extract_entries(data_dir, &backup_file, force)?;
                println!("Restore complete");

                Ok(())
            }
        }
    }
}

fn extract_entries(data_dir: &Path, backup_file: &Path, force: bool) -> anyhow::Result<()> {
    let backup_file = File::open(backup_file)?;
    let mut archive = tar::Archive::new(backup_file);
    for file in archive.entries()? {
        let mut file = file?;
        let path = file.path()?;
        let path = data_dir.join(path);
        if path.exists() && !force {
            bail!(
                "File already exists at {}. Use --force to overwrite.",
                path.display()
            );
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        println!("Restoring {}", path.display());
        file.unpack(path)?;
    }

    Ok(())
}

fn archive_entries(
    data_dir: &PathBuf,
    backup_entries: Vec<PathBuf>,
    backup_file_path: &Path,
) -> anyhow::Result<()> {
    let backup_file = File::create(backup_file_path)?;
    let mut archive = tar::Builder::new(backup_file);
    for entry in backup_entries {
        let entry_canonicalized = entry.canonicalize()?;
        let name = entry.strip_prefix(data_dir)?;

        println!("Adding {} to backup", entry_canonicalized.display());
        if entry_canonicalized.is_dir() {
            archive.append_dir_all(name, entry_canonicalized)?;
        } else {
            archive.append_path_with_name(entry_canonicalized, name)?;
        }
    }
    archive.into_inner()?;
    Ok(())
}

fn validate_and_add_entries(
    data_dir: &Path,
    no_keypair: bool,
    no_keystore: bool,
    backup_chain: Option<NetworkChain>,
    include_proof_params: bool,
) -> anyhow::Result<Vec<PathBuf>> {
    if no_keypair && no_keystore && backup_chain.is_none() && !include_proof_params {
        bail!("Nothing to backup!");
    }

    let mut valid = true;
    let mut backup_entries = vec![];

    if !no_keypair {
        let keypair_path = data_dir.join("libp2p").join("keypair");
        if keypair_path.exists() {
            backup_entries.push(keypair_path);
        } else {
            println!("Keypair not found at {}", keypair_path.display());
            valid = false;
        }
    }

    if !no_keystore {
        let mut any_keystore_found = false;
        for keystore in ["keystore.json", "keystore"].iter() {
            let keystore_path = data_dir.join(keystore);
            if keystore_path.exists() {
                backup_entries.push(keystore_path);
                any_keystore_found = true;
            }
        }
        if !any_keystore_found {
            println!("No keystore found at {}", data_dir.display());
            valid = false;
        }
    }

    if let Some(chain) = backup_chain {
        let chain_path = data_dir.join(chain.to_string());
        if chain_path.exists() {
            backup_entries.push(chain_path);
        } else {
            println!("Chain data not found at {}", chain_path.display());
            valid = false;
        }
    }

    if include_proof_params {
        let proof_params_path = data_dir.join("filecoin-proof-parameters");
        if proof_params_path.exists() {
            backup_entries.push(proof_params_path);
        } else {
            println!(
                "Proof parameters not found at {}",
                proof_params_path.display()
            );
            valid = false;
        }
    }

    if !valid {
        bail!("Backup aborted. Some files were not found.");
    }

    Ok(backup_entries)
}

#[cfg(test)]
mod test {
    use itertools::Itertools;
    use tempfile::TempDir;
    use walkdir::WalkDir;

    use super::*;

    #[test]
    fn validate_and_add_entries_no_entries() {
        let data_dir = PathBuf::from("test");
        let result = validate_and_add_entries(
            &data_dir, true,  // no_keypair
            true,  // no_keystore
            None,  // no backup_chain
            false, // no include_proof_params
        );
        assert!(result.is_err());
    }

    fn create_test_data() -> (TempDir, Vec<PathBuf>) {
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path().to_path_buf();
        let mut entries = vec![
            data_dir.join("libp2p").join("keypair"),
            data_dir.join("keystore.json"),
            data_dir.join("keystore"),
        ];

        entries.iter().for_each(|entry| {
            std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
            File::create(entry).unwrap();
        });

        let chain_path = data_dir.join("calibnet");
        std::fs::create_dir_all(&chain_path).unwrap();
        entries.push(chain_path);

        let proof_params_path = data_dir.join("filecoin-proof-parameters");
        std::fs::create_dir_all(&proof_params_path).unwrap();
        entries.push(proof_params_path);

        (temp_dir, entries)
    }

    #[test]
    fn validate_and_add_entries_all() {
        let (temp_dir, entries) = create_test_data();
        let data_dir = temp_dir.path().to_path_buf();

        let result = validate_and_add_entries(
            &data_dir,
            false,                        // include keypair
            false,                        // include keystore
            Some(NetworkChain::Calibnet), // backup the chain
            true,                         // include proof_params
        );

        let backup_entries = result.unwrap();
        itertools::assert_equal(entries.iter().sorted(), backup_entries.iter().sorted());
    }

    #[test]
    fn validate_and_add_entries_no_keypair() {
        let (temp_dir, _) = create_test_data();
        let data_dir = temp_dir.path().to_path_buf();

        std::fs::remove_file(data_dir.join("libp2p").join("keypair")).unwrap();

        let result = validate_and_add_entries(
            &data_dir,
            false,                        // include keypair
            false,                        // include keystore
            Some(NetworkChain::Calibnet), // backup the chain
            true,                         // include proof_params
        );
        assert!(result.is_err());

        let result = validate_and_add_entries(
            &data_dir,
            true,                         // exclude keypair
            false,                        // include keystore
            Some(NetworkChain::Calibnet), // backup the chain
            true,                         // include proof_params
        );
        assert!(result.is_ok());
    }

    #[test]
    fn validate_and_add_entries_no_keystore() {
        let (temp_dir, _) = create_test_data();
        let data_dir = temp_dir.path().to_path_buf();

        std::fs::remove_file(data_dir.join("keystore.json")).unwrap();
        let result = validate_and_add_entries(
            &data_dir,
            false,                        // include keypair
            false,                        // include keystore
            Some(NetworkChain::Calibnet), // backup the chain
            true,                         // include proof_params
        );
        // it should be fine - there is also the encrypted keystore
        assert!(result.is_ok());

        std::fs::remove_file(data_dir.join("keystore")).unwrap();
        let result = validate_and_add_entries(
            &data_dir,
            false,                        // include keypair
            false,                        // include keystore
            Some(NetworkChain::Calibnet), // backup the chain
            true,                         // include proof_params
        );
        assert!(result.is_err());

        let result = validate_and_add_entries(
            &data_dir,
            false,                        // include keypair
            true,                         // exclude keystore
            Some(NetworkChain::Calibnet), // backup the chain
            true,                         // include proof_params
        );
        assert!(result.is_ok());
    }

    #[test]
    fn validate_and_add_entries_proof_params() {
        let (temp_dir, _) = create_test_data();
        let data_dir = temp_dir.path().to_path_buf();

        std::fs::remove_dir_all(data_dir.join("filecoin-proof-parameters")).unwrap();
        let result = validate_and_add_entries(
            &data_dir,
            false,                        // include keypair
            false,                        // include keystore
            Some(NetworkChain::Calibnet), // backup the chain
            true,                         // include proof_params
        );
        assert!(result.is_err());

        let result = validate_and_add_entries(
            &data_dir,
            false,                        // include keypair
            false,                        // include keystore
            Some(NetworkChain::Calibnet), // backup the chain
            false,                        // exclude proof_params
        );
        assert!(result.is_ok());
    }

    #[test]
    fn validate_and_add_entries_no_chain() {
        let (temp_dir, _) = create_test_data();
        let data_dir = temp_dir.path().to_path_buf();

        std::fs::remove_dir_all(data_dir.join("calibnet")).unwrap();
        let result = validate_and_add_entries(
            &data_dir,
            false,                        // include keypair
            false,                        // include keystore
            Some(NetworkChain::Calibnet), // backup the chain
            true,                         // include proof_params
        );
        assert!(result.is_err());

        let result = validate_and_add_entries(
            &data_dir, false, // include keypair
            false, // include keystore
            None,  // no backup_chain
            true,  // include proof_params
        );
        assert!(result.is_ok());
    }

    #[test]
    fn archive_extract_roundtrip() {
        let (temp_dir, entries) = create_test_data();
        let data_dir = temp_dir.path().to_path_buf();

        let backup_file = tempfile::Builder::new().suffix(".tar").tempfile().unwrap();
        archive_entries(&data_dir, entries.clone(), backup_file.path()).unwrap();

        let restore_dir = tempfile::tempdir().unwrap();
        extract_entries(restore_dir.path(), backup_file.path(), true).unwrap();

        // get all entries recursively
        let get_entries_recurse = |dir| {
            WalkDir::new(dir)
                .into_iter()
                .filter_map(Result::ok)
                .map(|entry| entry.path().strip_prefix(dir).unwrap().to_path_buf())
                .sorted()
                .collect_vec()
        };
        let restored = get_entries_recurse(restore_dir.path());
        let original = get_entries_recurse(&data_dir);

        assert!(restored.len() > entries.len());
        itertools::assert_equal(original.iter(), restored.iter());
    }
}
