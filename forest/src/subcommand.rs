// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use auth::{create_token, ADMIN, JWT_IDENTIFIER};
use rpassword::read_password;
use rpc_client::API_INFO;
use std::path::PathBuf;
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
                    println!("Enter the keystore passphrase: ");

                    let passphrase = read_password().expect("Error reading passphrase");

                    let mut data_dir = PathBuf::from(&config.data_dir);
                    data_dir.push(ENCRYPTED_KEYSTORE_NAME);

                    if !data_dir.exists() {
                        println!("The keystore cannot be found from defaults, the environment, or provided arguments");
                    }

                    let key_store_init_result = KeyStore::new(KeyStoreConfig::Encrypted(
                        PathBuf::from(&config.data_dir),
                        passphrase,
                    ));

                    match key_store_init_result {
                        Ok(ks) => break ks,
                        Err(_) => {
                            log::error!("Incorrect passphrase entered.")
                        }
                    };
                }
            } else {
                KeyStore::new(KeyStoreConfig::Persistent(PathBuf::from(&config.data_dir)))
                    .expect("Error finding keystore")
            };

            let key_info = keystore
                .get(JWT_IDENTIFIER)
                .expect("Keystore initialized with a JWT private key");

            create_token(ADMIN.to_owned(), key_info.private_key())
                .expect("JWT private key parsed into a JWT")
        }
    };

    {
        let mut api_info = API_INFO.write().await;
        api_info.token = Some(token);
    }

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
        Subcommand::Net(cmd) => {
            cmd.run().await;
        }
        Subcommand::Wallet(cmd) => {
            cmd.run().await;
        }
        Subcommand::Sync(cmd) => {
            cmd.run().await;
        }
    }
}
