// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr;

use cid::Cid;
use forest_blocks::TipsetKeys;
use forest_json::cid::CidJson;
use forest_rpc_client::chain_ops::*;
use structopt::StructOpt;

use super::*;

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

    /// Prints a BLAKE2b hash of the tipset given its keys. Useful for setting
    /// checkpoints to speed up boot times from a snapshot
    TipsetHash { cids: Vec<String> },

    /// Runs through all epochs back to 0 and validates the tipset checkpoint
    /// hashes
    ValidateTipsetCheckpoints,

    /// Reads and prints out a message referenced by the specified CID from the
    /// chain block store
    Message {
        /// Input a valid CID
        #[structopt(short)]
        cid: String,
    },

    /// Reads and prints out IPLD nodes referenced by the specified CID from
    /// chain block store and returns raw bytes
    ReadObj {
        /// Input a valid CID
        #[structopt(short)]
        cid: String,
    },
}

impl ChainCommands {
    pub async fn run(&self, config: Config) -> anyhow::Result<()> {
        match self {
            Self::Block { cid } => {
                let cid: Cid = cid.parse()?;
                print_rpc_res_pretty(
                    chain_get_block((CidJson(cid),), &config.client.rpc_token).await,
                )
            }
            Self::Genesis => {
                print_rpc_res_pretty(chain_get_genesis(&config.client.rpc_token).await)
            }
            Self::Head => print_rpc_res_cids(chain_head(&config.client.rpc_token).await),
            Self::TipsetHash { cids } => {
                use forest_blocks::tipset_keys_json::TipsetKeysJson;

                let tipset_keys = TipsetKeys::new(
                    cids.iter()
                        .map(|s| Cid::from_str(s).expect("invalid cid"))
                        .collect(),
                );

                let tsk_json = TipsetKeysJson(tipset_keys);
                print_rpc_res(
                    chain_get_tipset_hash((tsk_json,), &config.client.rpc_token)
                        .await
                        .map(|s| format!("blake2b hash: {s}")),
                )
            }
            Self::ValidateTipsetCheckpoints => {
                let result = chain_validate_tipset_checkpoints((), &config.client.rpc_token).await;
                print_rpc_res(result)
            }
            Self::Message { cid } => {
                let cid: Cid = cid.parse()?;
                print_rpc_res_pretty(
                    chain_get_message((CidJson(cid),), &config.client.rpc_token).await,
                )
            }
            Self::ReadObj { cid } => {
                let cid: Cid = cid.parse()?;
                print_rpc_res(chain_read_obj((CidJson(cid),), &config.client.rpc_token).await)
            }
        }
    }
}
