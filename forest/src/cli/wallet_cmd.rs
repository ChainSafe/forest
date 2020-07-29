// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use rpc_client::{balance, default, export, import, list, new, new_client, set_default, verify};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub enum WalletCommands {
    /// Prints out wallet balance of specified address via RPC
    #[structopt(about = "Get account balance")]
    Balance {
        #[structopt(help = "Input a valid address")]
        address: String,
    },

    /// Creates wallet of specified signature type via RPC
    #[structopt(
        about = "Generate a new key of the given type",
        help = "Generate a new key of the given type"
    )]
    New {
        #[structopt(help = "Input signature type (BLS | SECP256k1)")]
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
        #[structopt(help = "Input signature type (BLS | SECP256k1)")]
        address: String,
    },

    /// Prints out the address marked as default in the wallet via RPC
    #[structopt(
        name = "default",
        about = "Get default wallet address",
        help = "Get default wallet address"
    )]
    Def,

    // Identifies if the signature is valid via RPC
    // #[structopt(about = "Verify the signature of a message", help = "Verify the signature of a message")]
    // Verify {
    //     address: String,
    //     signature: String,
    //     sig_type: Vec<u8>
    // },
    /// Imports key info into wallet via RPC
    #[structopt(about = "Imports wallet keys", help = "Imports wallet keys")]
    Import {
        #[structopt(help = "Input key info")]
        key_info: KeyInfo,
    },

    /// Prints out private key of specified address in wallet via RPC
    #[structopt(about = "Exports wallet keys", help = "Exports wallet keys")]
    Export {
        #[structopt(help = "Input valid address")]
        address: String,
    },
}

impl WalletCommands {
    pub async fn run(&self) {
        // TODO handle cli config
        match self {
            Self::Balance { address } => {
                let client = new_client();

                let balance = balance(client, address.to_string()).await;
                println!("{}", serde_json::to_string_pretty(&balance).unwrap());
            }
            Self::New { sig_type } => {
                let client = new_client();

                let addr = new(client, sig_type.as_bytes().to_vec()).await;
                println!("{}", serde_json::to_string_pretty(&addr).unwrap());
            }
            Self::List => {
                let client = new_client();

                let addresses = list(client).await;
                println!("{}", serde_json::to_string_pretty(&addresses).unwrap());
            }
            Self::SetDefault { address } => {
                let client = new_client();

                let default = set_default(client, address.to_string()).await;
                println!("{}", serde_json::to_string_pretty(&default).unwrap());
            }
            Self::Def => {
                let client = new_client();

                let def = default(client).await;
                println!("{}", serde_json::to_string_pretty(&def).unwrap());
            }
            Self::Import { key_info } => {
                let client = new_client();

                let imp = import(client, key_info).await;
                println!("{}", serde_json::to_string_pretty(&imp).unwrap());
            }
            Self::Export { address } => {
                let client = new_client();

                let exp = export(client, address).await;
                println!("{}", serde_json::to_string_pretty(&exp).unwrap());
            }
            // Self::Verify { address, signature, sig_type } => {
            //     let client = new_client();

            //     let sig = verify(client);
            // },
        }
    }
}
