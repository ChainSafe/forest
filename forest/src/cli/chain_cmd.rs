// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use rpc_client::{genesis, head, messages};
use cid::Cid;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct ChainCommands {
    /// Prints out the genesis tipset
    #[structopt(long, help = "Prints genesis tipset")]
    pub genesis: bool,

    /// Prints out the canonical head of the chain
    #[structopt(long, help = "Print chain head")]
    pub head: bool,

    /// Reads and prints out ipld nodes referenced by the specified CID from chain
	/// blockstore and returns raw bytes
    #[structopt(
        long = "read-obj",
        value_name = "CID",
        help = "Read the raw bytes of an object"
    )]
    pub read_obj: Option<String>,

    /// Reads and prints out a message referenced by the specified CID from the
	/// chain blockstore.
    #[structopt(
        long = "message",
        value_name = "CIDs",
        help = "Retrieves and prints messages by CIDs"
    )]
    pub messages: Option<String>,

    /// Retrieves and prints out the block specified by the given CID
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
            let gen = genesis().await;
            println!("{}", serde_json::to_string_pretty(&gen).unwrap());
        }
        if self.head {
            let head = head().await;
            println!("{}", serde_json::to_string_pretty(&head).unwrap());
        }
        if let Some(params) = &self.messages {
            let cid: Cid = params.parse().unwrap();
            let msg = messages(cid).await;
            println!("{}", serde_json::to_string_pretty(&msg).unwrap());
        }
    }
}
