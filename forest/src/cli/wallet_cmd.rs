// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use rpc_client::{new_client, new, balance, list, set_default};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub enum WalletCommands {
    /// Retrieves and prints out the block specified by the given CID
    #[structopt(about = "Get account balance")]
    Balance {
        #[structopt(help = "Input a valid address")]
        address: String,
    },

    /// Prints out the genesis tipset
    #[structopt(about = "Generate a new key of the given type", help = "Generate a new key of the given type")]
    New {
        #[structopt(help = "Input signature type (BLS | SECP256k1)")]
        sig_type: String,
    },

    /// Prints out the canonical head of the chain
    #[structopt(about = "List wallet address", help = "List wallet address")]
    List,

    /// Prints out the genesis tipset
    #[structopt(about = "Set default wallet address", help = "Set default wallet address")]
    SetDefault {
        #[structopt(help = "Input signature type (BLS | SECP256k1)")]
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
            Self::New{ sig_type } => {
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
        }
    }
}
