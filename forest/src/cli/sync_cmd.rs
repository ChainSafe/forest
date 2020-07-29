

use structopt::StructOpt;
use rpc_client::{new_client, mark_bad, check_bad, status};
use rpc::RPCSyncState;
use cid::{json::CidJson, Cid};
use std::time::SystemTime;

#[derive(Debug, StructOpt)]
pub enum SyncCommand{

    #[structopt(
        name = "mark-bad",
        about = "Mark the given block as bad, will prevent syncing to a chain that contains it"
    )]
    MarkBad {
        #[structopt(short, long, help = "Block Cid given as string argument")]
        block_cid : String
    },

    #[structopt(
        name = "check-bad",
        about = "Check if the given block was marked bad, and for what reason"
    )]
    CheckBad {
        #[structopt(short, long, help = "Block Cid given as string argument")]
        block_cid : String
    },

    #[structopt(
        name = "status",
        about = "Check sync status"
    )]
    Status,

    #[structopt(
        name = "wait",
        about = "Wait for sync to be complete"
    )]
    Wait
}

impl SyncCommand {
    pub async fn run (self){

        let mut client = new_client();

        match self {
            SyncCommand::Status{} => {
                println!("In status sub command.");
                let response = status(client).await;
                if let Ok(r) = response {
                    println!("sync status:");
                    for (i, active_sync) in r.active_syncs.iter().enumerate() {
                        println!("Worker {}:", i);
                        let height_diff = 0;
                        let height = 0;
            
                        let mut base: Option<Vec<Cid>> = None;
                        let mut target: Option<Vec<Cid>> = None;
            
                        if let Some(b) = &active_sync.base {
                            base = Some(b.cids().to_vec());
                            //height_diff = 0;
                        }
            
                        if let Some(b) = &active_sync.target {
                            target = Some(b.cids().to_vec());
                            //height_diff = 0;
                        }
            
                        println!("\tBase:\t{:?}\n", base.unwrap_or(vec![]));
                        println!(
                            "\tTarget:\t{:?} Height:\t({})\n",
                            target.unwrap_or(vec![]),
                            height
                        );
                        println!("\tHeight diff:\t{}\n", height_diff);
                        println!("\tStage: {}\n", active_sync.stage());
                        if let Some(end_time) = active_sync.end {
                            if let Some(start_time) = active_sync.start {
                                if end_time == SystemTime::UNIX_EPOCH {
                                    if start_time != SystemTime::UNIX_EPOCH {
                                        println!(
                                            "\tElapsed: {:?}\n",
                                            start_time.duration_since(SystemTime::UNIX_EPOCH).unwrap()
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
            }
            SyncCommand::Wait{} => {
                println!("In wait sub command.");
                //async_std::task::block_on(handle_wait(client));
            }
            SyncCommand::MarkBad{block_cid}  => {
                println!("In mark-bad sub command. Block is {:?}", block_cid);
                let response = mark_bad(client, block_cid.clone()).await;
                if response.is_ok(){
                    println!("Successfully marked block {} as bad", block_cid);
                }
                else {
                    println!("Failed to mark block {} as bad", block_cid);
                }
            }
            SyncCommand::CheckBad{block_cid}  => {
                println!("In check-bad sub command. Block is {:?}", block_cid);
                let response = check_bad(client, block_cid.clone()).await;
                if let Ok(reason) = response {
                    println!("Block {} is bad because \"{}\"", block_cid, reason);
                }
                else {
                    println!("Failed to check if block {} is bad", block_cid);
                }
            }
        }
    }
}
