// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::stringify_rpc_err;
use rpc_client::{balance, default, export, list, new, new_client, set_default, sign};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub enum WalletCommands {
    /// Prints out wallet balance of specified address via RPC
    #[structopt(about = "Get account balance")]
    Balance {
        #[structopt(short, help = "Input a valid address")]
        address: String,
    },

    /// Creates wallet of specified signature type via RPC
    #[structopt(
        about = "Generate a new key of the given type",
        help = "Generate a new key of the given type"
    )]
    New {
        #[structopt(
            short,
            default_value = "secp256k1",
            help = "Input signature type [bls | secp256k1] (default secp256k1)"
        )]
        sig_type: String,
    },

    /// Prints out all addresses in the wallet via RPC
    #[structopt(about = "List wallet address", help = "List wallet address")]
    List,

    /// Marks the given address as as the default wallet via RPC
    #[structopt(
        about = "Set default wallet address",
        help = "Set default wallet address"
    )]
    SetDefault {
        #[structopt(short, help = "Input valid address")]
        address: String,
    },

    /// Prints out the address marked as default in the wallet via RPC
    #[structopt(
        name = "default",
        about = "Get default wallet address",
        help = "Get default wallet address"
    )]
    Def,

    /// Prints out private key of specified address in wallet via RPC
    #[structopt(about = "Exports wallet keys", help = "Exports wallet keys")]
    Export {
        #[structopt(short, help = "Input valid address")]
        address: String,
    },

    /// Signs the given bytes using the given address via RPC
    #[structopt(about = "Sign a message", help = "Sign a message")]
    Sign {
        #[structopt(short, help = "Must specify signing address")]
        address: String,
        #[structopt(short, help = "Must specify message to sign")]
        message: String,
    },
}

impl WalletCommands {
    pub async fn run(&self) {
        // TODO add verify and import cmds
        match self {
            Self::Balance { address } => {
                let client = new_client();

                let balance = balance(client, address.to_string())
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{}", balance);
            }
            Self::New { sig_type } => {
                let client = new_client();

                let sig = sig_type.parse().unwrap();
                let addr = new(client, sig).await.map_err(stringify_rpc_err).unwrap();
                println!("{}", addr);
            }
            Self::List => {
                let client = new_client();

                let addresses = list(client).await.map_err(stringify_rpc_err).unwrap();
                println!("{:?}", addresses);
            }
            Self::SetDefault { address } => {
                let client = new_client();

                set_default(client, address.to_string())
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("Default wallet address set: {}", address.to_string());
            }
            Self::Def => {
                let client = new_client();

                let def = default(client).await.map_err(stringify_rpc_err).unwrap();
                println!("{}", def);
            }
            Self::Export { address } => {
                let client = new_client();

                let exp = export(client, address.to_string())
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{}", serde_json::to_string_pretty(&exp).unwrap());
            }
            Self::Sign { address, message } => {
                let client = new_client();

                let signed = sign(client, (address.to_string(), message.to_string()))
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{}", serde_json::to_string_pretty(&signed).unwrap());
            }
        }
    }
}
