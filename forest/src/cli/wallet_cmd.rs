// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use rpc_client::{block, genesis, head, messages, new_client, read_obj};
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
    New,

    /// Prints out the canonical head of the chain
    #[structopt(about = "Print chain head", help = "Print chain head")]
    Head,

    /// Reads and prints out a message referenced by the specified CID from the
    /// chain blockstore
    #[structopt(about = "<CID> Retrieves and prints messages by CIDs")]
    Message {
        #[structopt(help = "Input a valid CID")]
        cid: String,
    },

    /// Reads and prints out ipld nodes referenced by the specified CID from chain
    /// blockstore and returns raw bytes
    #[structopt(about = "<CID> Read the raw bytes of an object")]
    ReadObj {
        #[structopt(help = "Input a valid CID")]
        cid: String,
    },
}

impl WalletCommands {
    pub async fn run(&self) {
        // TODO handle cli config
        match self {
            Self::Block { cid } => {
                let cid: Cid = cid.parse().unwrap();
                let mut client = new_client();

                let blk = block(client, cid).await;
                println!("{}", serde_json::to_string_pretty(&blk).unwrap());
            }
            Self::Genesis => {
                let mut client = new_client();

                let gen = genesis(client).await;
                println!("{}", serde_json::to_string_pretty(&gen).unwrap());
            }
            Self::Head => {
                let mut client = new_client();

                let head = head(client).await;
                println!("{}", serde_json::to_string_pretty(&head).unwrap());
            }
            Self::Message { cid } => {
                let cid: Cid = cid.parse().unwrap();
                let mut client = new_client();

                let msg = messages(client, cid).await;
                println!("{}", serde_json::to_string_pretty(&msg).unwrap());
            }
            Self::ReadObj { cid } => {
                let cid: Cid = cid.parse().unwrap();
                let mut client = new_client();

                let obj = read_obj(client, cid).await;
                println!("{}", serde_json::to_string_pretty(&obj).unwrap());
            }
        }
    }
}
