// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{print_rpc_res_cids, print_rpc_res_pretty};
use cid::Cid;
use rpc_client::{block, genesis, head, messages, read_obj};
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
        match self {
            Self::Block { cid } => {
                let cid: Cid = cid.parse().unwrap();
                print_rpc_res_pretty(block(cid).await);
            }
            Self::Genesis => {
                print_rpc_res_pretty(genesis().await);
            }
            Self::Head => {
                print_rpc_res_cids(head().await);
            }
            Self::Message { cid } => {
                let cid: Cid = cid.parse().unwrap();
                print_rpc_res_pretty(messages(cid).await);
            }
            Self::ReadObj { cid } => {
                let cid: Cid = cid.parse().unwrap();
                print_rpc_res_pretty(read_obj(cid).await);
            }
        }
    }
}
