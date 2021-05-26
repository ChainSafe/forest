// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_crypto::signature::{json::signature_type::SignatureTypeJson, SignatureType};
use rpc_client::{new_client, wallet_ops};
use structopt::StructOpt;

use super::stringify_rpc_err;

#[derive(Debug, StructOpt)]
pub enum WalletCommands {
    #[structopt(about = "Create a new wallet")]
    New {
        #[structopt(
            short,
            help = "The signature type to use. One of Secp256k1, or BLS. Defaults to BLS"
        )]
        signature_type: String,
    },
    #[structopt(about = "Create a new wallet")]
    Balance,
    #[structopt(about = "Get the default address of the wallet")]
    Default,
    #[structopt(about = "Export the wallet's keys")]
    Export,
    #[structopt(about = "Check if the wallet has a key")]
    Has {
        #[structopt(short, help = "The key to check")]
        key: String,
    },
    #[structopt(about = "import keys from existing wallet")]
    Import {
        #[structopt(short, help = "specify input format for key (default: hex-lotus)")]
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
    },
    #[structopt(about = "Verify the signature of a message")]
    Verify {
        #[structopt(about = "The message to verify", short)]
        message: String,
    },
}

impl WalletCommands {
    pub async fn run(&self) {
        let mut client = new_client();

        match self {
            Self::New { signature_type } => {
                let signature_type = match signature_type.as_str() {
                    "secp256k1" => SignatureType::Secp256k1,
                    _ => SignatureType::BLS,
                };

                let signature_type_json = SignatureTypeJson(signature_type);

                let response = wallet_ops::wallet_new(&mut client, signature_type_json)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Balance => {
                let response = wallet_ops::wallet_balance(&mut client)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Default => {
                let response = wallet_ops::wallet_default_address(&mut client)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Export => {
                let response = wallet_ops::wallet_export(&mut client)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{:#?}", response);
            }
            Self::Has { key } => {
                let key = key.parse().unwrap();
                let response = wallet_ops::wallet_has(&mut client, key)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
            Self::Import { format, as_default } => {
                println!("format: {}", format);
                println!("as default: {}", as_default);
            }
            Self::List => {
                let response = wallet_ops::wallet_list(&mut client)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{:#?}", response);
            }
            Self::SetDefault { key } => {
                let key = key.parse().unwrap();
                wallet_ops::wallet_set_default(&mut client, key)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
            }
            Self::Sign { message } => {
                let message = message.parse().unwrap();
                let response = wallet_ops::wallet_sign(&mut client, message)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{:#?}", response);
            }
            Self::Verify { message } => {
                let message = message.parse().unwrap();
                let response = wallet_ops::wallet_verify(&mut client, message)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{}", response);
            }
        };
    }
}
