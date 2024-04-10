// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::{Tipset, TipsetKey};
use crate::lotus_json::{HasLotusJson, LotusJson};
use crate::message::ChainMessage;
use crate::rpc::{self, prelude::*};
use anyhow::bail;
use cid::Cid;
use clap::Subcommand;
use nonempty::NonEmpty;

use super::{print_pretty_json, print_rpc_res_cids};

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
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Block { cid } => {
                print_pretty_json(ChainGetBlock::call(&client, (cid.into(),)).await?)
            }
            Self::Genesis => print_pretty_json(ChainGetGenesis::call(&client, ()).await?),
            Self::Head => print_rpc_res_cids(ChainHead::call(&client, ()).await?.into_inner()),
            Self::Message { cid } => {
                let bytes = ChainReadObj::call(&client, (cid.into(),))
                    .await?
                    .into_inner();
                match fvm_ipld_encoding::from_slice::<ChainMessage>(&bytes)? {
                    ChainMessage::Unsigned(m) => print_pretty_json(LotusJson(m)),
                    ChainMessage::Signed(m) => {
                        let cid = m.cid()?;
                        print_pretty_json(m.into_lotus_json().with_cid(cid))
                    }
                }
            }
            Self::ReadObj { cid } => {
                let bytes = ChainReadObj::call(&client, (cid.into(),))
                    .await?
                    .into_inner();
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
                ChainSetHead::call(&client, (LotusJson(tipset.key().into()),)).await?;
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
                    (LotusJson(
                        TipsetKey::from(
                            NonEmpty::from_vec(cids).expect("empty vec disallowed by clap"),
                        )
                        .into(),
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
    let current_head = ChainHead::call(client, ()).await?.into_inner();

    let target_epoch = match epoch_or_offset.is_negative() {
        true => current_head.epoch() + epoch_or_offset, // adding negative number
        false => epoch_or_offset,
    };
    Ok(ChainGetTipSetByHeight::call(
        client,
        (target_epoch, LotusJson(current_head.key().clone().into())),
    )
    .await?
    .into_inner())
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
