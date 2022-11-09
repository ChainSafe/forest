// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Config;
use forest_json::address::json::AddressJson;
use forest_json::signature::json::{signature_type::SignatureTypeJson, SignatureJson};
use forest_key_management::json::KeyInfoJson;
use forest_rpc_client::wallet_ops::*;
use forest_utils::io::read_file_to_string;
use fvm_shared::address::Address;
use fvm_shared::bigint::BigInt;
use fvm_shared::crypto::signature::{Signature, SignatureType};
use fvm_shared::econ::TokenAmount;
use rpassword::read_password;
use std::{
    path::PathBuf,
    str::{self, FromStr},
};
use structopt::StructOpt;

use super::{cli_error_and_die, handle_rpc_err};

#[derive(Debug, StructOpt)]
pub enum WalletCommands {
    /// Create a new wallet
    New {
        /// The signature type to use. One of SECP256k1, or BLS
        #[structopt(default_value = "secp256k1")]
        signature_type: String,
    },
    /// Get account balance
    Balance {
        /// The address of the account to check
        address: String,
    },
    /// Get the default address of the wallet
    Default,
    /// Export the wallet's keys
    Export {
        /// The address that contains the keys to export
        address: String,
    },
    /// Check if the wallet has a key
    Has {
        /// The key to check
        key: String,
    },
    /// Import keys from existing wallet
    Import {
        /// The path to the private key
        path: Option<String>,
    },
    /// List addresses of the wallet
    List,
    /// Set the default wallet address
    SetDefault {
        /// The given key to set to the default address
        key: String,
    },
    /// Sign a message
    Sign {
        /// The hex encoded message to sign
        #[structopt(short)]
        message: String,
        /// The address to be used to sign the message
        #[structopt(short)]
        address: String,
    },
    /// Verify the signature of a message. Returns true if the signature matches the message and address
    Verify {
        /// The address used to sign the message
        #[structopt(short)]
        address: String,
        /// The message to verify
        #[structopt(short)]
        message: String,
        /// The signature of the message to verify
        #[structopt(short)]
        signature: String,
    },
}

impl WalletCommands {
    pub async fn run(&self, config: Config) {
        match self {
            Self::New { signature_type } => {
                let signature_type = match signature_type.to_lowercase().as_str() {
                    "secp256k1" => SignatureType::Secp256k1,
                    _ => SignatureType::BLS,
                };

                let signature_type_json = SignatureTypeJson(signature_type);

                let response = wallet_new((signature_type_json,), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Balance { address } => {
                let response = wallet_balance((address.to_string(),), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Default => {
                let response = wallet_default_address(&config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Export { address } => {
                let response = wallet_export((address.to_string(),), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();

                let encoded_key = serde_json::to_string(&response).unwrap();
                println!("{}", hex::encode(encoded_key))
            }
            Self::Has { key } => {
                let response = wallet_has((key.to_string(),), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Import { path } => {
                let key = match path {
                    Some(path) => match read_file_to_string(&PathBuf::from(path)) {
                        Ok(key) => key,
                        _ => cli_error_and_die(format!("{path} is not a valid path"), 1),
                    },
                    _ => {
                        println!("Enter the private key: ");
                        read_password().expect("Error reading private key")
                    }
                };

                let key = key.trim();

                let decoded_key_result = hex::decode(key);

                if decoded_key_result.is_err() {
                    cli_error_and_die("Key must be hex encoded", 1);
                }

                let decoded_key = decoded_key_result.unwrap();

                let key_str = str::from_utf8(&decoded_key).unwrap();

                let key_result: Result<KeyInfoJson, serde_json::error::Error> =
                    serde_json::from_str(key_str);

                if key_result.is_err() {
                    cli_error_and_die(format!("{key} is not a valid key to import"), 1);
                }

                let key = key_result.unwrap();

                let key = wallet_import(vec![KeyInfoJson(key.0)], &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();

                println!("{}", key);
            }
            Self::List => {
                let response = wallet_list(&config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();

                let default = match wallet_default_address(&config.client.rpc_token).await {
                    Ok(addr) => addr,
                    Err(err) => {
                        println!("Failed get the wallet default address");
                        handle_rpc_err(err);
                    }
                };

                let (title_address, title_default_mark, title_balance) =
                    ("Address", "Default", "Balance");
                println!("{title_address:41} {title_default_mark:7} {title_balance}");

                for address in response {
                    let addr = address.0.to_string();
                    let default_address_mark = if addr == default { "X" } else { "" };

                    let balance_string =
                        match wallet_balance((addr.clone(),), &config.client.rpc_token).await {
                            Ok(balance) => balance,
                            Err(err) => {
                                println!("Failed loading the wallet balance");
                                handle_rpc_err(err);
                            }
                        };

                    let balance_int = match balance_string.parse::<BigInt>() {
                        Ok(balance) => TokenAmount::from_atto(balance),
                        Err(err) => {
                            println!(
                                "Couldn't convert balance {} to TokenAmount: {}",
                                balance_string, err
                            );
                            continue;
                        }
                    };

                    println!("{addr:41}  {default_address_mark:7}  {balance_int:.6} FIL");
                }
            }
            Self::SetDefault { key } => {
                let key_parse_result = Address::from_str(key);

                if key_parse_result.is_err() {
                    cli_error_and_die("Error parsing address. Verify that the address exists and is in the keystore", 1);
                }

                let key = key_parse_result.unwrap();

                let key_json = AddressJson(key);
                wallet_set_default((key_json,), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
            }
            Self::Sign { address, message } => {
                let address_result = Address::from_str(address);

                if address_result.is_err() {
                    cli_error_and_die(format!("{address} is not a valid address"), 1);
                }

                let address = address_result.unwrap();

                let message = hex::decode(message).unwrap();
                let message = base64::encode(message);

                let response = wallet_sign(
                    (AddressJson(address), message.into_bytes()),
                    &config.client.rpc_token,
                )
                .await
                .map_err(handle_rpc_err)
                .unwrap();
                println!("{}", hex::encode(response.0.bytes()));
            }
            Self::Verify {
                message,
                address,
                signature,
            } => {
                let sig_bytes = hex::decode(signature).unwrap();
                let signature = match address.chars().nth(1).unwrap() {
                    '1' => Signature::new_secp256k1(sig_bytes),
                    '3' => Signature::new_bls(sig_bytes),
                    _ => cli_error_and_die("Invalid signature (must be bls or secp256k1)", 1),
                };

                let response = wallet_verify(
                    (
                        address.to_string(),
                        message.to_string(),
                        SignatureJson(signature),
                    ),
                    &config.client.rpc_token,
                )
                .await
                .map_err(handle_rpc_err)
                .unwrap();

                println!("{}", response);
            }
        };
    }
}
