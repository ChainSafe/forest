// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_crypto::{
    signature::{json::signature_type::SignatureTypeJson, SignatureType},
    Signature,
};
use rpc_client::wallet_ops;
use structopt::StructOpt;

use super::handle_rpc_err;

#[derive(Debug, StructOpt)]
pub enum WalletCommands {
    #[structopt(about = "Create a new wallet")]
    New {
        #[structopt(
            short,
            default_value = "bls",
            help = "The signature type to use. One of Secp256k1, or bls"
        )]
        signature_type: String,
    },
    #[structopt(about = "Get account balance")]
    Balance {
        #[structopt(about = "The address to of the account to check", short)]
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
        #[structopt(short, help = "The key to check")]
        key: String,
    },
    #[structopt(about = "import keys from existing wallet")]
    Import {
        #[structopt(
            short,
            default_value = "hex-lotus",
            help = "specify input format for key"
        )]
        format: String,
        #[structopt(short, help = "import the given key as your new default key")]
        as_default: bool,
    },
    #[structopt(about = "List addresses of the wallet")]
    List,
    #[structopt(about = "Set the defualt wallet address")]
    SetDefault {
        #[structopt(about = "The given key to set to the default address", short)]
        key: String,
    },
    #[structopt(about = "Sign a message")]
    Sign {
        #[structopt(about = "The message to sign", short)]
        message: String,
        #[structopt(about = "The address to be used to sign the message", short)]
        signing_address: String,
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
                println!("{:#?}", response);
            }
            Self::Has { key } => {
                let key = key.parse().unwrap();
                let response = wallet_ops::wallet_has(key)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Import { format, as_default } => {
                println!("format: {}", format);
                println!("as default: {}", as_default);
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
            Self::Sign {
                message,
                signing_address,
            } => {
                let message = ();
                let response = wallet_ops::wallet_sign(signing_address.to_string(), message)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                println!("{:#?}", response);
            }
            Self::Verify {
                message,
                address,
                signature,
            } => {
                let signature = Signature {
                    sig_type: val,
                    bytes: val,
                };
                let response = wallet_ops::wallet_verify(message, address, signature)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
        };
    }
}
