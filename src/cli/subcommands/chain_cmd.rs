// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::{Tipset, TipsetKey};
use crate::lotus_json::HasLotusJson;
use crate::message::ChainMessage;
use crate::rpc::{self, prelude::*};
use anyhow::{bail, ensure};
use cid::Cid;
use clap::Subcommand;
use nunny::Vec as NonEmpty;

use super::{print_pretty_lotus_json, print_rpc_res_cids};

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
    Head {
        /// Print the first `n` tipsets from the head (inclusive).
        /// Tipsets are categorized by epoch in descending order.
        #[arg(short = 'n', long, default_value = "1")]
        tipsets: u64,
    },

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
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Block { cid } => {
                print_pretty_lotus_json(ChainGetBlock::call(&client, (cid,)).await?)
            }
            Self::Genesis => print_pretty_lotus_json(ChainGetGenesis::call(&client, ()).await?),
            Self::Head { tipsets } => print_chain_head(&client, tipsets).await,
            Self::Message { cid } => {
                let bytes = ChainReadObj::call(&client, (cid,)).await?;
                match fvm_ipld_encoding::from_slice::<ChainMessage>(&bytes)? {
                    ChainMessage::Unsigned(m) => print_pretty_lotus_json(m),
                    ChainMessage::Signed(m) => {
                        let cid = m.cid();
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&m.into_lotus_json().with_cid(cid))?
                        );
                        Ok(())
                    }
                }
            }
            Self::ReadObj { cid } => {
                let bytes = ChainReadObj::call(&client, (cid,)).await?;
                println!("{}", hex::encode(bytes));
                Ok(())
            }
            Self::SetHead {
                cids,
                epoch: Some(epoch),
                force: no_confirm,
            } => {
                maybe_confirm(no_confirm, SET_HEAD_CONFIRMATION_MESSAGE)?;
                assert!(cids.is_empty(), "should be disallowed by clap");
                let tipset = tipset_by_epoch_or_offset(&client, epoch).await?;
                ChainSetHead::call(&client, (tipset.key().clone(),)).await?;
                Ok(())
            }
            Self::SetHead {
                cids,
                epoch: None,
                force: no_confirm,
            } => {
                maybe_confirm(no_confirm, SET_HEAD_CONFIRMATION_MESSAGE)?;
                ChainSetHead::call(
                    &client,
                    (TipsetKey::from(
                        NonEmpty::new(cids).expect("empty vec disallowed by clap"),
                    ),),
                )
                .await?;
                Ok(())
            }
        }
    }
}

/// If `epoch_or_offset` is negative, get the tipset that many blocks before the
/// current head. Else treat `epoch_or_offset` as an epoch, and get that tipset.
async fn tipset_by_epoch_or_offset(
    client: &rpc::Client,
    epoch_or_offset: i64,
) -> Result<Tipset, jsonrpsee::core::ClientError> {
    let current_head = ChainHead::call(client, ()).await?;

    let target_epoch = match epoch_or_offset.is_negative() {
        true => current_head.epoch() + epoch_or_offset, // adding negative number
        false => epoch_or_offset,
    };
    ChainGetTipSetByHeight::call(client, (target_epoch, current_head.key().clone().into())).await
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

/// Print the first `n` tipsets from the head (inclusive).
async fn print_chain_head(client: &rpc::Client, n: u64) -> anyhow::Result<()> {
    ensure!(n > 0, "number of tipsets must be positive");
    let current_epoch = ChainHead::call(client, ()).await?.epoch() as u64;

    for epoch in (current_epoch.saturating_sub(n - 1)..=current_epoch).rev() {
        let tipset = tipset_by_epoch_or_offset(client, epoch.try_into()?).await?;
        println!("[{}]", epoch);
        print_rpc_res_cids(tipset)?;
    }
    Ok(())
}
