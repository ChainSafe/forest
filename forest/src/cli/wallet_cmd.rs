// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use address::{json::AddressJson, Address};
use forest_crypto::{
    signature::{
        json::{signature_type::SignatureTypeJson, SignatureJson},
        SignatureType,
    },
    Signature,
};
use rpc_client::*;
use structopt::StructOpt;
use wallet::json::KeyInfoJson;

use super::{cli_error_and_die, handle_rpc_err};

#[derive(Debug, StructOpt)]
pub enum WalletCommands {
    #[structopt(about = "Create a new wallet")]
    New {
        #[structopt(
            default_value = "secp256k1",
            help = "The signature type to use. One of secp256k1, or bls"
        )]
        signature_type: String,
    },
    #[structopt(about = "Get account balance")]
    Balance {
        #[structopt(about = "The address of the account to check")]
        address: String,
    },
    #[structopt(about = "Get the default address of the wallet")]
    Default,
    #[structopt(about = "Export the wallet's keys")]
    Export {
        #[structopt(about = "The address that contains the keys to export")]
        address: String,
    },
    #[structopt(about = "Check if the wallet has a key")]
    Has {
        #[structopt(help = "The key to check")]
        key: String,
    },
    #[structopt(about = "Import keys from existing wallet")]
    Import {
        #[structopt(help = "The key to import")]
        key: String,
    },
    #[structopt(about = "List addresses of the wallet")]
    List,
    #[structopt(about = "Set the default wallet address")]
    SetDefault {
        #[structopt(about = "The given key to set to the default address")]
        key: String,
    },
    #[structopt(about = "Sign a message")]
    Sign {
        #[structopt(about = "The hex encoded message to sign", short)]
        message: String,
        #[structopt(about = "The address to be used to sign the message", short)]
        address: String,
    },
    #[structopt(
        about = "Verify the signature of a message. Returns true if the signature matches the message and address"
    )]
    Verify {
        #[structopt(about = "The address used to sign the message", short)]
        address: String,
        #[structopt(about = "The message to verify", short)]
        message: String,
        #[structopt(about = "The signature of the message to verify", short)]
        signature: String,
    },
}

impl WalletCommands {
    pub async fn run(&self) {
        match self {
            Self::New { signature_type } => {
                let signature_type = match signature_type.to_lowercase().as_str() {
                    "secp256k1" => SignatureType::Secp256k1,
                    _ => SignatureType::BLS,
                };

                let signature_type_json = SignatureTypeJson(signature_type);

                let response = wallet_new((signature_type_json,))
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Balance { address } => {
                let response = wallet_balance((address.to_string(),))
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Default => {
                let response = wallet_default_address()
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Export { address } => {
                let response = wallet_export((address.to_string(),))
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();

                let encoded_key = serde_json::to_string(&response).unwrap();
                println!("{}", hex::encode(encoded_key))
            }
            Self::Has { key } => {
                let response = wallet_has((key.to_string(),))
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Import { key } => {
                use std::str;
                let decoded_key_result = hex::decode(key);

                if decoded_key_result.is_err() {
                    cli_error_and_die("Key must be hex encoded", 1);
                }

                let decoded_key = decoded_key_result.unwrap();

                let key_str = str::from_utf8(&decoded_key).unwrap();

                let key_result: Result<KeyInfoJson, serde_json::error::Error> =
                    serde_json::from_str(&key_str);

                if key_result.is_err() {
                    cli_error_and_die(&format!("{} is not a valid key to import", key), 1);
                }

                let key = key_result.unwrap();

                let _ = wallet_import(vec![KeyInfoJson(key.0)])
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
            }
            Self::List => {
                let response = wallet_list().await.map_err(handle_rpc_err).unwrap();

                response.iter().for_each(|address| {
                    println!("{}", address.0);
                });
            }
            Self::SetDefault { key } => {
                let key = Address::from_str(&key.to_string()).unwrap();
                let key_json = AddressJson(key);
                wallet_set_default((key_json,))
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
            }
            Self::Sign { address, message } => {
                let address_result = Address::from_str(address);

                if address_result.is_err() {
                    cli_error_and_die(&format!("{} is not a valid address", address), 1);
                }

                let address = address_result.unwrap();

                let message = hex::decode(message).unwrap();
                let message = base64::encode(message);

                let response = wallet_sign((AddressJson(address), message.into_bytes()))
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
                    _ => {
                        return cli_error_and_die(
                            "Invalid signature (must be bls or secp256k1)",
                            1,
                        );
                    }
                };

                let response = wallet_verify((
                    address.to_string(),
                    message.to_string(),
                    SignatureJson(signature),
                ))
                .await
                .map_err(handle_rpc_err)
                .unwrap();

                println!("{}", response);
            }
        };
    }
}
