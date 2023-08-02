// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::TipsetKeys;
use crate::json::cid::CidJson;
use crate::rpc_client::chain_ops::*;
use anyhow::bail;
use cid::Cid;
use clap::Subcommand;
use futures::TryFutureExt;

use super::*;

#[derive(Debug, Subcommand)]
pub enum ChainCommands {
    /// Retrieves and prints out the block specified by the given CID
    Block {
        #[arg(short)]
        cid: Cid,
    },

    /// Prints out the genesis tipset
    Genesis,

    /// Prints out the canonical head of the chain
    Head,

    /// Reads and prints out a message referenced by the specified CID from the
    /// chain block store
    Message {
        #[arg(short)]
        cid: Cid,
    },

    /// Reads and prints out IPLD nodes referenced by the specified CID from
    /// chain block store and returns raw bytes
    ReadObj {
        #[arg(short)]
        cid: Cid,
    },

    /// Manually set the head to the given tipset. This invalidates blocks
    /// between the desired head and the new head
    SetHead {
        /// Construct the new head tipset from these CIDs
        #[arg(num_args = 1.., required = true)]
        cids: Vec<Cid>,
        /// Use the tipset from this epoch as the new head.
        /// Negative numbers specify decrements from the current head.
        #[arg(long, conflicts_with = "cids", allow_hyphen_values = true)]
        epoch: Option<i64>,
        /// Skip confirmation dialogue.
        #[arg(short, long, aliases = ["yes", "no-confirm"], short_alias = 'y')]
        force: bool,
    },
}

impl ChainCommands {
    pub async fn run(&self, config: Config) -> anyhow::Result<()> {
        match self {
            Self::Block { cid } => print_rpc_res_pretty(
                chain_get_block((CidJson(*cid),), &config.client.rpc_token).await,
            ),
            Self::Genesis => {
                print_rpc_res_pretty(chain_get_genesis(&config.client.rpc_token).await)
            }
            Self::Head => print_rpc_res_cids(chain_head(&config.client.rpc_token).await),
            Self::Message { cid } => print_rpc_res_pretty(
                chain_get_message((CidJson(*cid),), &config.client.rpc_token).await,
            ),
            Self::ReadObj { cid } => {
                print_rpc_res(chain_read_obj((CidJson(*cid),), &config.client.rpc_token).await)
            }
            Self::SetHead {
                cids,
                epoch: Some(epoch),
                force: no_confirm,
            } => {
                maybe_confirm(*no_confirm, SET_HEAD_CONFIRMATION_MESSAGE)?;
                assert!(cids.is_empty(), "should be disallowed by clap");
                tipset_by_epoch_or_offset(*epoch, &config.client.rpc_token)
                    .and_then(|tipset| {
                        chain_set_head((tipset.0.key().clone(),), &config.client.rpc_token)
                    })
                    .await
                    .map_err(handle_rpc_err)
            }
            Self::SetHead {
                cids,
                epoch: None,
                force: no_confirm,
            } => {
                maybe_confirm(*no_confirm, SET_HEAD_CONFIRMATION_MESSAGE)?;
                chain_set_head((TipsetKeys::from(cids.clone()),), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)
            }
        }
    }
}

/// If `epoch_or_offset` is negative, get the tipset that many blocks before the
/// current head. Else treat `epoch_or_offset` as an epoch, and get that tipset.
async fn tipset_by_epoch_or_offset(
    epoch_or_offset: i64,
    auth_token: &Option<String>,
) -> Result<TipsetJson, JsonRpcError> {
    let current_head = chain_head(auth_token).await?;

    let target_epoch = match epoch_or_offset.is_negative() {
        true => current_head.0.epoch() + epoch_or_offset, // adding negative number
        false => epoch_or_offset,
    };

    chain_get_tipset_by_height((target_epoch, current_head.0.key().clone()), auth_token).await
}

const SET_HEAD_CONFIRMATION_MESSAGE: &str =
    "Manually setting head is an unsafe operation that could brick the node! Continue?";

fn maybe_confirm(no_confirm: bool, prompt: impl Into<String>) -> anyhow::Result<()> {
    if no_confirm {
        return Ok(());
    }
    let should_continue = dialoguer::Confirm::new()
        .default(false)
        .with_prompt(prompt)
        .wait_for_newline(true)
        .interact()?;
    match should_continue {
        true => Ok(()),
        false => bail!("Operation cancelled by user"),
    }
}
