// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use jsonrpc_v2::Error as JsonRpcError;
use log::warn;
use rpc_client::{block, genesis, head, messages, new_client, read_obj};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub enum ChainCommands {
    /// Retrieves and prints out the block specified by the given CID
    #[structopt(about = "<Cid> Retrieve a block and print its details")]
    Block {
        #[structopt(help = "Input a valid CID")]
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

impl ChainCommands {
    pub async fn run(&self) {
        // TODO handle cli config
        match self {
            Self::Block { cid } => {
                let cid: Cid = cid.parse().unwrap();
                let client = new_client();

                let blk = block(client, cid)
                    .await
                    .map_err(|e| {
                        stringify_rpc_err(e);
                    })
                    .unwrap();
                println!("{}", serde_json::to_string_pretty(&blk).unwrap());
            }
            Self::Genesis => {
                let client = new_client();

                let gen = genesis(client)
                    .await
                    .map_err(|e| {
                        stringify_rpc_err(e);
                    })
                    .unwrap();
                println!("{}", serde_json::to_string_pretty(&gen).unwrap());
            }
            Self::Head => {
                let client = new_client();

                let canonical = head(client)
                    .await
                    .map_err(|e| {
                        stringify_rpc_err(e);
                    })
                    .unwrap();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&canonical.0.cids()).unwrap()
                );
            }
            Self::Message { cid } => {
                let cid: Cid = cid.parse().unwrap();
                let client = new_client();

                let msg = messages(client, cid)
                    .await
                    .map_err(|e| {
                        stringify_rpc_err(e);
                    })
                    .unwrap();
                println!("{}", serde_json::to_string_pretty(&msg).unwrap());
            }
            Self::ReadObj { cid } => {
                let cid: Cid = cid.parse().unwrap();
                let client = new_client();

                let obj = read_obj(client, cid)
                    .await
                    .map_err(|e| {
                        stringify_rpc_err(e);
                    })
                    .unwrap();
                println!("{}", serde_json::to_string_pretty(&obj).unwrap());
            }
        }
    }
}

fn stringify_rpc_err(e: JsonRpcError) {
    match e {
        JsonRpcError::Full {
            code,
            message,
            data: _,
        } => {
            return warn!("JSON RPC Error: Code: {} Message: {}", code, message);
        }
        JsonRpcError::Provided { code, message } => {
            return warn!("JSON RPC Error: Code: {} Message: {}", code, message);
        }
    }
}
