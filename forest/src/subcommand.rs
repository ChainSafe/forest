// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use auth::{create_token, ADMIN, JWT_IDENTIFIER};
use rpassword::read_password;
use rpc_client::API_INFO;
use std::{io::Write, path::PathBuf};
use wallet::{KeyStore, KeyStoreConfig, ENCRYPTED_KEYSTORE_NAME};

use super::cli::{Config, Subcommand};

/// Process CLI subcommand
pub(super) async fn process(command: Subcommand, config: Config) {
    let token = match config.rpc_token.to_owned() {
        Some(token) => token,
        // If no token argument is passed or configured, attempt to load it from the local keystore
        None => {
            let keystore = if config.encrypt_keystore {
                loop {
                    print!("keystore passphrase: ");
                    std::io::stdout().flush().unwrap();

                    let passphrase = read_password().expect("Error reading passphrase");

                    let mut data_dir = PathBuf::from(&config.data_dir);
                    data_dir.push(ENCRYPTED_KEYSTORE_NAME);

                    if !data_dir.exists() {
                        print!("keystore cannot be found from environment or arguments");
                        std::io::stdout().flush().unwrap();

                        if passphrase != read_password().unwrap() {
                            println!("passphrases do not match. please retry");
                            continue;
                        }
                    }

                    let key_store_init_result = KeyStore::new(KeyStoreConfig::Encrypted(
                        PathBuf::from(&config.data_dir),
                        passphrase,
                    ));

                    match key_store_init_result {
                        Ok(ks) => break ks,
                        Err(_) => {
                            log::error!("incorrect passphrase")
                        }
                    };
                }
            } else {
                KeyStore::new(KeyStoreConfig::Persistent(PathBuf::from(&config.data_dir)))
                    .expect("Error initializing keystore")
            };

            let key_info = keystore
                .get(JWT_IDENTIFIER)
                .expect("Keystore initialized with a JWT private key");

            create_token(ADMIN.to_owned(), key_info.private_key())
                .expect("JWT private key parsed into a JWT")
        }
    };

    let mut api_info = API_INFO.write().await;
    api_info.token = Some(token);

    // Run command
    match command {
        Subcommand::Fetch(cmd) => {
            cmd.run().await;
        }
        Subcommand::Chain(cmd) => {
            cmd.run().await;
        }
        Subcommand::Auth(cmd) => {
            cmd.run(config).await;
        }
        Subcommand::Genesis(cmd) => {
            cmd.run().await;
        }
        Subcommand::Wallet(cmd) => {
            cmd.run().await;
        }
    }
}
