// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use rpc_client::{get_genesis, get_head};
use structopt::StructOpt;

#[allow(missing_docs)]
#[derive(Debug, StructOpt)]
pub struct ChainCommands {
    /// Prints out the genesis tipset
    #[structopt(long, help = "Prints genesis tipset")]
    pub genesis: bool,

    /// Prints out the canonical head of the chain
    #[structopt(long, help = "Print chain head")]
    pub head: bool,

    #[structopt(
        long = "read-obj",
        value_name = "CID",
        help = "Read the raw bytes of an object"
    )]
    pub read_obj: Option<String>,

    #[structopt(
        long = "message",
        value_name = "CIDs",
        help = "Retrieve and print messages by CIDs"
    )]
    pub messages: Option<String>,

    #[structopt(
        long = "block",
        value_name = "CID",
        help = "Retrieve a block and print its details"
    )]
    pub block: Option<String>,
}

impl ChainCommands {
    pub async fn run(&self) {
        if self.genesis {
            let gen = get_genesis().await;
            println!("{}", serde_json::to_string_pretty(&gen).unwrap());
        }
        if self.head {
            let head = get_head().await;
            println!("{}", serde_json::to_string_pretty(&head).unwrap());
        }
    }
}
