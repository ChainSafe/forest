// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{json::CidJson, Cid};
use rpc_client::*;
use structopt::StructOpt;

use crate::cli::handle_rpc_err;

use super::print_rpc_res;

#[derive(Debug, StructOpt)]
pub enum SyncCommands {
    #[structopt(about = "Wait for sync to be complete")]
    Wait,
    #[structopt(about = "Check sync status")]
    Status,
    #[structopt(about = "Check if a given block is marked bad, and for what reason")]
    CheckBad {
        #[structopt(short, about = "the block CID to check")]
        cid: String,
    },
    #[structopt(about = "Mark a given block as bad")]
    MarkBad {
        #[structopt(short, about = "the block CID to mark as a bad block")]
        cid: String,
    },
}

impl SyncCommands {
    pub async fn run(&self) {
        match self {
            Self::Wait => {}
            Self::Status => {
                let response = status(()).await.map_err(handle_rpc_err).unwrap();

                let state = &response.active_syncs[0];
                let base = state.base();
                let elapsed_time = state.get_elapsed_time();
                let target = state.target();

                let (target_cids, target_height) = if let Some(tipset) = target {
                    (tipset.cids().to_vec(), tipset.epoch())
                } else {
                    (vec![], 0)
                };

                let (base_cids, base_height) = if let Some(tipset) = base {
                    (tipset.cids().to_vec(), tipset.epoch())
                } else {
                    (vec![], 0)
                };

                let height_diff = target_height - base_height;

                let hex_target_cids: Vec<String> = target_cids
                    .iter()
                    .map(|cid| hex::encode(cid.to_bytes()))
                    .collect();

                let hex_base_cids: Vec<String> = base_cids
                    .iter()
                    .map(|cid| hex::encode(cid.to_bytes()))
                    .collect();

                println!("sync status:");
                println!("Base:\t{:?}", hex_base_cids);
                println!("Target:\t{:?} ({})", hex_target_cids, target_height);
                println!("Height diff:\t{}", height_diff);

                if let Some(duration) = elapsed_time {
                    println!("Elapsed time:\t{}", duration);
                }
            }
            Self::CheckBad { cid } => {
                let cid: Cid = cid.parse().unwrap();
                print_rpc_res(check_bad((CidJson(cid),)).await);
            }
            Self::MarkBad { cid } => {
                let cid: Cid = cid.parse().unwrap();
                match mark_bad((CidJson(cid),)).await {
                    Ok(()) => println!("OK"),
                    Err(error) => handle_rpc_err(error),
                }
            }
        }
    }
}
