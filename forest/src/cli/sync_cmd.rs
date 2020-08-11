// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::stringify_rpc_err;
use actor::EPOCH_DURATION_SECONDS;
use chain_sync::get_naive_time_now;
use chrono::naive::NaiveDateTime;
use chrono::prelude::*;
use cid::Cid;
use jsonrpc_v2::Error as JsonRpcError;
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient as HTC;
use rpc_client::{check_bad, head, mark_bad, new_client, status, submit_block};
use std::thread;
use std::time::Duration;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub enum SyncCommand {
    #[structopt(
        name = "mark-bad",
        about = "Mark the given block as bad, will prevent syncing to a chain that contains it"
    )]
    MarkBad {
        #[structopt(help = "Block Cid given as string argument")]
        block_cid: String,
    },

    #[structopt(
        name = "check-bad",
        about = "Check if the given block was marked bad, and for what reason"
    )]
    CheckBad {
        #[structopt(help = "Block Cid given as string argument")]
        block_cid: String,
    },

    #[structopt(
        name = "submit",
        about = "Submit newly created block to network through node"
    )]
    Submit {
        #[structopt(help = "Gossip block as String argument")]
        gossip_block: String,
    },

    #[structopt(name = "status", about = "Check sync status")]
    Status,

    #[structopt(name = "wait", about = "Wait for sync to be complete")]
    Wait,
}

fn get_naive_time_zero() -> NaiveDateTime {
    NaiveDate::from_ymd(1, 1, 1).and_hms(0, 0, 0)
}

impl SyncCommand {
    pub async fn run(self) {
        let mut client = new_client();

        match self {
            SyncCommand::Status {} => {
                let response = status(&mut client).await;
                if let Ok(r) = response {
                    println!("sync status:");
                    for (i, active_sync) in r.active_syncs.iter().enumerate() {
                        println!("Worker {}:", i);
                        let mut height_diff = 0;
                        let height = 0;

                        let mut base: Option<Vec<Cid>> = None;
                        let mut target: Option<Vec<Cid>> = None;

                        if let Some(b) = &active_sync.base {
                            base = Some(b.cids().to_vec());
                            height_diff = b.epoch();
                        }

                        if let Some(b) = &active_sync.target {
                            target = Some(b.cids().to_vec());
                            height_diff = b.epoch() - height_diff;
                        } else {
                            height_diff = 0;
                        }

                        println!("\tBase:\t{:?}", base.unwrap_or_default());
                        println!(
                            "\tTarget:\t{:?} Height:\t({})",
                            target.unwrap_or_default(),
                            height
                        );
                        println!("\tHeight diff:\t{}", height_diff);
                        println!("\tStage: {}", active_sync.stage());
                        println!("\tHeight: {}", active_sync.epoch);
                        if let Some(end_time) = active_sync.end {
                            if let Some(start_time) = active_sync.start {
                                let zero_time = get_naive_time_zero();

                                if end_time == zero_time {
                                    if start_time != zero_time {
                                        let time_now = get_naive_time_now();
                                        println!(
                                            "\tElapsed: {:?}\n",
                                            time_now.signed_duration_since(start_time)
                                        );
                                    }
                                } else {
                                    println!(
                                        "\tElapsed: {:?}\n",
                                        end_time.signed_duration_since(start_time)
                                    );
                                }
                            }
                        }
                    }
                }
            }

            SyncCommand::Wait {} => {
                loop {
                    // If not done syncing or runs into a error stop waiting
                    if sync_wait(&mut client).await.unwrap_or(true) {
                        break;
                    }
                }
            }

            SyncCommand::MarkBad { block_cid } => {
                let response = mark_bad(&mut client, block_cid.clone()).await;
                if response.is_ok() {
                    println!("Successfully marked block {} as bad", block_cid);
                } else {
                    println!("Failed to mark block {} as bad, error is {} ", block_cid, stringify_rpc_err(response.unwrap_err()));
                }
            }

            SyncCommand::CheckBad { block_cid } => {
                let response = check_bad(&mut client, block_cid.clone()).await;
                if let Ok(reason) = response {
                    println!("Block {} is \"{}\"", block_cid, reason);
                } else {
                    println!("Failed to check if block {} is bad", block_cid);
                }
            }
            SyncCommand::Submit { gossip_block } => {
                let response = submit_block(&mut client, gossip_block).await;
                if response.is_ok() {
                    println!("Successfully submitted block");
                } else {
                    println!(
                        "Did not submit block because {:#?}",
                        stringify_rpc_err(response.unwrap_err())
                    );
                }
            }
        }
    }
}

//TODO : This command hasn't been completed in Lotus. Needs to be updated
async fn sync_wait(client: &mut RawClient<HTC>) -> Result<bool, JsonRpcError> {
    let state = status(client).await?;
    let head = head(client).await?;

    let mut working = 0;
    for (i, _active_sync) in state.active_syncs.iter().enumerate() {
        // TODO update this loop when lotus adds logic
        working = i;
    }

    let ss = &state.active_syncs[working];
    let mut target: Option<Vec<Cid>> = None;
    if let Some(ss_target) = &ss.target {
        target = Some(ss_target.cids().to_vec());
    }

    println!(
        "\r\x1b[2KWorker {}: Target: {:?}\tState: {}\tHeight: {}",
        working,
        target,
        ss.stage(),
        ss.epoch
    );

    let time_diff = get_naive_time_now().timestamp() - head.0.min_timestamp() as i64;

    if time_diff < EPOCH_DURATION_SECONDS {
        println!("Done");
        return Ok(true);
    }
    thread::sleep(Duration::from_secs(3));
    Ok(false)
}
