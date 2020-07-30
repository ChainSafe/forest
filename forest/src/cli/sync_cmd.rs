// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{json::CidJson, Cid};
use rpc::RPCSyncState;
use rpc_client::{check_bad, mark_bad, new_client, status, submit_block, head};
use std::time::SystemTime;
use structopt::StructOpt;
use super::stringify_rpc_err;

#[derive(Debug, StructOpt)]
pub enum SyncCommand {
    #[structopt(
        name = "mark-bad",
        about = "Mark the given block as bad, will prevent syncing to a chain that contains it"
    )]
    MarkBad {
        #[structopt(short, long, help = "Block Cid given as string argument")]
        block_cid: String,
    },

    #[structopt(
        name = "check-bad",
        about = "Check if the given block was marked bad, and for what reason"
    )]
    CheckBad {
        #[structopt(short, long, help = "Block Cid given as string argument")]
        block_cid: String,
    },

    #[structopt(
        name = "submit",
        about = "Submit newly created block to network through node"
    )]
    Submit {
        #[structopt(short, long, help = "Gossip block as String argument")]
        gossip_block: String,
    },

    #[structopt(name = "status", about = "Check sync status")]
    Status,

    #[structopt(name = "wait", about = "Wait for sync to be complete")]
    Wait,
}

impl SyncCommand {
    pub async fn run(self) {
        let mut client = new_client();

        match self {
            SyncCommand::Status {} => {
                println!("In status sub command.");
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
                        }
                        else{
                            height_diff = 0;
                        }

                        println!("\tBase:\t{:?}\n", base.unwrap_or(vec![]));
                        println!(
                            "\tTarget:\t{:?} Height:\t({})\n",
                            target.unwrap_or(vec![]),
                            height
                        );
                        println!("\tHeight diff:\t{}\n", height_diff);
                        println!("\tStage: {}\n", active_sync.stage());
                        println!("\tHeight: {}\n", active_sync.epoch);
                        if let Some(end_time) = active_sync.end {
                            if let Some(start_time) = active_sync.start {
                                if end_time == SystemTime::UNIX_EPOCH {
                                    if start_time != SystemTime::UNIX_EPOCH {
                                        println!(
                                            "\tElapsed: {:?}\n",
                                            start_time
                                                .duration_since(SystemTime::UNIX_EPOCH)
                                                .unwrap()
                                        );
                                    }
                                } else {
                                    println!(
                                        "\tElapsed: {:?}\n",
                                        end_time.duration_since(start_time).unwrap()
                                    );
                                }
                            }
                        }
                    }
                }
            },

            //TODO : This command hasn't been completed in Lotus. Needs to be updated
            SyncCommand::Wait {} => {
                println!("In wait sub command.");
                let response = status(&mut client).await;
                if let Ok(state) = response {

                    //client = new_client();
                    let head_result = head(&mut client).await;

                    if let Ok(head) = head_result {

                        let mut working = 0;
                        for (i, active_sync) in state.active_syncs.iter().enumerate() {
                            // TODO update this loop when lotus adds logic
                            working = i;
                        }

                        let ss = &state.active_syncs[working];
                        let mut target : Option<Vec<Cid>> = None;
                        if let Some(ss_target) = &ss.target {
                            target = Some(ss_target.cids().to_vec());
                        }

                        println!("\r\x1b[2KWorker {}: Target: {:?}\tState: {}\tHeight: {}", working, target, ss.stage(), ss.epoch);
                        


                    }
                    else{
                        println!("Unable to get Chain Head");
                    }
                }
                else{
                    println!("Unable to retrieve state from the daemon");
                }
            },

            SyncCommand::MarkBad { block_cid } => {
                println!("In mark-bad sub command. Block is {:?}", block_cid);
                let response = mark_bad(&mut client, block_cid.clone()).await;
                if response.is_ok() {
                    println!("Successfully marked block {} as bad", block_cid);
                } else {
                    println!("Failed to mark block {} as bad", block_cid);
                }
            },

            SyncCommand::CheckBad { block_cid } => {
                println!("In check-bad sub command. Block is {:?}", block_cid);
                let response = check_bad(&mut client, block_cid.clone()).await;
                if let Ok(reason) = response {
                    println!("Block {} is bad because \"{}\"", block_cid, reason);
                } else {
                    println!("Failed to check if block {} is bad", block_cid);
                }
            }
            SyncCommand::Submit { gossip_block } => {
                println!("In submit command. Gossip Block is {:?}", gossip_block);
                let response = submit_block(&mut client, gossip_block).await;
                if response.is_ok() {
                    println!("Successfully submitted block");
                }
                else {
                    println!("Did not submit block because {:#?}", stringify_rpc_err (response.unwrap_err()) );
                }
            }
        }
    }
}
