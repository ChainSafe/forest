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
                println!("{:?}", response.0.active_syncs);
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
