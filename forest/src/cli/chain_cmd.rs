// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::stringify_rpc_err;
use cid::Cid;
use rpc_client::{block, genesis, head, messages, new_client, read_obj};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub enum ChainCommands {
    /// Retrieves and prints out the block specified by the given CID
    #[structopt(about = "<Cid> Retrieve a block and print its details")]
    Block {
        #[structopt(short, help = "Input a valid CID")]
        cid: String,
    },

    /// Prints out the genesis tipset
    #[structopt(about = "Prints genesis tipset", help = "Prints genesis tipset")]
    Genesis,

    /// Prints out the canonical head of the chain
    #[structopt(about = "Print chain head", help = "Print chain head")]
    Head,

    /// Reads and prints out a message referenced by the specified CID from the
    /// chain blockstore
    #[structopt(about = "<CID> Retrieves and prints messages by CIDs")]
    Message {
        #[structopt(short, help = "Input a valid CID")]
        cid: String,
    },

    /// Reads and prints out ipld nodes referenced by the specified CID from chain
    /// blockstore and returns raw bytes
    #[structopt(about = "<CID> Read the raw bytes of an object")]
    ReadObj {
        #[structopt(short, help = "Input a valid CID")]
        cid: String,
    },
}

impl ChainCommands {
    pub async fn run(&self) {
        // TODO handle cli config
        match self {
            Self::Block { cid } => {
                let cid: Cid = cid.parse().unwrap();
                let mut client = new_client();

                let blk = block(&mut client, cid)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{}", serde_json::to_string_pretty(&blk).unwrap());
            }
            Self::Genesis => {
                let mut client = new_client();

                let gen = genesis(&mut client)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{}", serde_json::to_string_pretty(&gen).unwrap());
            }
            Self::Head => {
                let mut client = new_client();

                let canonical = head(&mut client).await.map_err(stringify_rpc_err).unwrap();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&canonical.0.cids()).unwrap()
                );
            }
            Self::Message { cid } => {
                let cid: Cid = cid.parse().unwrap();
                let mut client = new_client();

                let msg = messages(&mut client, cid)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{}", serde_json::to_string_pretty(&msg).unwrap());
            }
            Self::ReadObj { cid } => {
                let cid: Cid = cid.parse().unwrap();
                let mut client = new_client();

                let obj = read_obj(&mut client, cid)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{}", serde_json::to_string_pretty(&obj).unwrap());
            }
        }
    }
}
