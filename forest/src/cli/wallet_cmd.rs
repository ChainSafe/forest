// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use address::Address;
use forest_crypto::{
    signature::{json::signature_type::SignatureTypeJson, SignatureType},
    Signature,
};
use rpc_client::wallet_ops;
use structopt::StructOpt;
use wallet::KeyInfo;

use super::handle_rpc_err;

#[derive(Debug, StructOpt)]
pub enum WalletCommands {
    #[structopt(about = "Create a new wallet")]
    New {
        #[structopt(
            default_value = "bls",
            help = "The signature type to use. One of Secp256k1, or bls"
        )]
        signature_type: String,
    },
    #[structopt(about = "Get account balance")]
    Balance {
        #[structopt(about = "The address to of the account to check")]
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
    #[structopt(about = "import keys from existing wallet")]
    Import {
        #[structopt(help = "The key to import")]
        key: String,
    },
    #[structopt(about = "List addresses of the wallet")]
    List,
    #[structopt(about = "Set the defualt wallet address")]
    SetDefault {
        #[structopt(about = "The given key to set to the default address")]
        key: String,
    },
    #[structopt(about = "Sign a message")]
    Sign {
        #[structopt(about = "The message to sign", short)]
        message: String,
        #[structopt(about = "The address to be used to sign the message", short)]
        address: String,
    },
    #[structopt(about = "Verify the signature of a message")]
    Verify {
        #[structopt(about = "The signing address", short)]
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

                let response = wallet_ops::wallet_new(signature_type_json)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Balance { address } => {
                let response = wallet_ops::wallet_balance(address.to_string())
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Default => {
                let response = wallet_ops::wallet_default_address()
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Export { address } => {
                let response = wallet_ops::wallet_export(address.to_string())
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                println!("{}", hex::encode(response.0.private_key()))
            }
            Self::Has { key } => {
                let key = key.parse().unwrap();
                let response = wallet_ops::wallet_has(key)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Import { key } => {
                use std::str;
                let decoded_key = hex::decode(key).unwrap();

                let key_str = str::from_utf8(&decoded_key).unwrap();

                println!("key_str: {}", key_str);

                let key: KeyInfo = serde_json::from_str(key_str).unwrap();

                let _ = wallet_ops::wallet_import(key)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
            }
            Self::List => {
                let response = wallet_ops::wallet_list()
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();

                response.iter().for_each(|address| {
                    println!("{}", address.0);
                });
            }
            Self::SetDefault { key } => {
                let key = key.parse().unwrap();
                wallet_ops::wallet_set_default(key)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
            }
            Self::Sign { address, message } => {
                let address = Address::from_str(address).unwrap();

                let message = base64::encode(message);

                let response = wallet_ops::wallet_sign(address, message.as_bytes().to_vec())
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
                let signature = match address.chars().nth(1).unwrap() {
                    '1' => Signature::new_secp256k1(signature.as_bytes().to_vec()),
                    '3' => Signature::new_bls(signature.as_bytes().to_vec()),
                    _ => {
                        println!("unimplemented signature type (must be bls or secp256k1)");
                        std::process::exit(1);
                    }
                };

                let response =
                    wallet_ops::wallet_verify(message.to_string(), address.to_string(), signature)
                        .await
                        .map_err(handle_rpc_err)
                        .unwrap();
                println!("{}", response);
            }
        };
    }
}
