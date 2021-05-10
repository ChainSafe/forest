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
    DefaultAddress,
    #[structopt(about = "Export the wallet's keys")]
    Export,
    #[structopt(about = "Check if the wallet has a key")]
    Has,
    #[structopt(about = "import keys from existing wallet")]
    Import,
    #[structopt(about = "List addresses of the wallet")]
    List,
    #[structopt(about = "Set the defualt wallet address")]
    SetDefault,
    #[structopt(about = "Sign a message")]
    Sign,
    #[structopt(about = "Verify the signature of a message")]
    Verify,
}

impl WalletCommands {
    pub async fn run(&self) {
        match self {
            Self::New { signature_type } => {
                let signature_type = match signature_type.as_str() {
                    "secp256k1" => SignatureType::Secp256k1,
                    _ => SignatureType::BLS,
                };

                let signature_type_json = SignatureTypeJson(signature_type);

                let mut client = new_client();

                let obj = wallet_ops::wallet_new(&mut client, signature_type_json)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{}", obj);
            }
            Self::Balance => {}
            Self::DefaultAddress => {
                let mut client = new_client();

                let obj = wallet_ops::wallet_default_address(&mut client)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{}", obj);
            }
            Self::Export => {}
            Self::Has => {}
            Self::Import => {}
            Self::List => {}
            Self::SetDefault => {}
            Self::Sign => {}
            Self::Verify => {}
        }
    }
}
