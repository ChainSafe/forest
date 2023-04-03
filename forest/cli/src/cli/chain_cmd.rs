// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use clap::Subcommand;
use forest_json::cid::CidJson;
use forest_rpc_client::chain_ops::*;
use forest_shim::clock::ChainEpoch;

use super::*;

#[derive(Debug, Subcommand)]
pub enum ChainCommands {
    /// Retrieves and prints out the block specified by the given CID
    Block {
        /// Input a valid CID
        #[arg(short)]
        cid: String,
    },

    /// Prints out the genesis tipset
    Genesis,

    /// Prints out the canonical head of the chain
    Head,

    /// Prints the checksum hash for a given epoch. This is used internally to
    /// improve performance when loading a snapshot.
    TipsetHash { epoch: Option<ChainEpoch> },

    /// Runs through all epochs back to 0 and validates the tipset checkpoint
    /// hashes
    ValidateTipsetCheckpoints,

    /// Reads and prints out a message referenced by the specified CID from the
    /// chain block store
    Message {
        /// Input a valid CID
        #[arg(short)]
        cid: String,
    },

    /// Reads and prints out IPLD nodes referenced by the specified CID from
    /// chain block store and returns raw bytes
    ReadObj {
        /// Input a valid CID
        #[arg(short)]
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
            Self::TipsetHash { epoch } => {
                use forest_blocks::tipset_keys_json::TipsetKeysJson;

                let TipsetJson(head) = chain_head(&config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;
                // Use the given epoch or HEAD-1. We can't use HEAD since more
                // blocks are likely to be received (changing the checkpoint hash)
                let target_epoch = epoch.unwrap_or(head.epoch() - 1);
                let TipsetJson(target) = chain_get_tipset_by_height(
                    (target_epoch, head.key().clone()),
                    &config.client.rpc_token,
                )
                .await
                .map_err(handle_rpc_err)?;
                let tipset_keys = target.key().clone();

                let tsk_json = TipsetKeysJson(tipset_keys);

                let checkpoint_hash = chain_get_tipset_hash((tsk_json,), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;
                println!("Chain:           {}", config.chain.name);
                println!("Epoch:           {}", target_epoch);
                println!("Checkpoint hash: {}", checkpoint_hash);
                Ok(())
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
