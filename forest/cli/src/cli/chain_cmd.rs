// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use structopt::StructOpt;

use super::*;
use cid::Cid;
use forest_json::cid::CidJson;
use forest_rpc_client::chain_ops::*;

#[derive(Debug, StructOpt)]
pub enum ChainCommands {
    /// Retrieves and prints out the block specified by the given CID
    Block {
        /// Input a valid CID
        #[structopt(short)]
        cid: String,
    },

    /// Prints out the genesis tipset
    Genesis,

    /// Prints out the canonical head of the chain
    Head,

    /// Reads and prints out a message referenced by the specified CID from the
    /// chain block store
    Message {
        /// Input a valid CID
        #[structopt(short)]
        cid: String,
    },

    /// Reads and prints out IPLD nodes referenced by the specified CID from chain
    /// block store and returns raw bytes
    ReadObj {
        /// Input a valid CID
        #[structopt(short)]
        cid: String,
    },
}

impl ChainCommands {
    pub async fn run(&self, config: Config) {
        match self {
            Self::Block { cid } => {
                let cid: Cid = cid.parse().unwrap();
                print_rpc_res_pretty(
                    chain_get_block((CidJson(cid),), &config.client.rpc_token).await,
                );
            }
            Self::Genesis => {
                print_rpc_res_pretty(chain_get_genesis(&config.client.rpc_token).await);
            }
            Self::Head => {
                print_rpc_res_cids(chain_head(&config.client.rpc_token).await);
            }
            Self::Message { cid } => {
                let cid: Cid = cid.parse().unwrap();
                print_rpc_res_pretty(
                    chain_get_message((CidJson(cid),), &config.client.rpc_token).await,
                );
            }
            Self::ReadObj { cid } => {
                let cid: Cid = cid.parse().unwrap();
                print_rpc_res(chain_read_obj((CidJson(cid),), &config.client.rpc_token).await);
            }
        }
    }
}
