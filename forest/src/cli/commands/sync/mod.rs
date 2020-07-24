// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::CLICommand;
use crate::sub_cmd;
use async_std;
use async_trait::async_trait;
use cid::*;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use jsonrpsee;
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient;
use rpc::RPCSyncState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use structopt::StructOpt;
use std::time::SystemTime;
use cid::json::CidJson;

sub_cmd!(
    SyncCommand, "Subcommands", "Subcommands for inspecting or interacting with the chain syncer" =>
        Status, "status", "Check sync status",
        Wait, "wait", "Wait for sync to be complete",
        MarkBad, "mark-bad",  "Mark the given block as bad, will prevent syncing to a chain that contains it",
        CheckBad, "check-bad", "Check if the given block was marked bad, and for what reason",
);

#[derive(StructOpt)]
pub struct Status {}

#[derive(StructOpt)]
pub struct Wait {}

#[derive(StructOpt)]
pub struct MarkBad {
    #[structopt(short, long)]
    block_cid: String,
}

#[derive(StructOpt)]
pub struct CheckBad {
    #[structopt(short, long)]
    block_cid: String,
}

jsonrpsee::rpc_api! {

    SyncApi  {
        #[rpc(method = "Filecoin.SyncState")]
        fn status() -> RPCSyncState ;

        #[rpc(method = "Filecoin.SyncMarkBad")]
        fn mark_bad(p :  CidJson)  ;

        #[rpc(method = "Filecoin.SyncCheckBad")]
        fn check_bad(p :  CidJson)  -> String;
    }
}

#[async_trait]
impl CLICommand for SyncCommand {
    async fn handle(self) {
        let transport_client = HttpTransportClient::new("http://127.0.0.1:1234/rpc/v0");
        let mut client = RawClient::new(transport_client);

        match self {
            SyncCommand::Status(_) => {
                println!("In status sub command.");
                async_std::task::block_on(handle_status(client));
            }
            SyncCommand::Wait(_) => {
                println!("In wait sub command.");
                
            },
            SyncCommand::MarkBad(b) => {
                println!("In mark-bad sub command. Block is {:?}", b.block_cid);
                async_std::task::block_on(handle_mark_bad(client, b.block_cid));

            }
            SyncCommand::CheckBad(b) => {
                println!("In check-bad sub command. Block is {:?}", b.block_cid);
                async_std::task::block_on(handle_check_bad(client, b.block_cid));
            }
        }
    }
}

async fn handle_status(mut client: RawClient<HttpTransportClient>) {
    if let Ok(r) = SyncApi::status(&mut client).await {
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
            println!("\tTarget:\t{:?} Height:\t({})\n", target.unwrap_or(vec![]), height);
            println!("\tHeight diff:\t{}\n", height_diff);
            println!("\tStage: {}\n", active_sync.stage());
            if let Some(end_time) = active_sync.end{
                if let Some(start_time) = active_sync.start{

                    if end_time == SystemTime::UNIX_EPOCH {
                        if start_time != SystemTime::UNIX_EPOCH{
                            println!("\tElapsed: {:?}\n", start_time.duration_since(SystemTime::UNIX_EPOCH).unwrap() );
                        }
                    }
                    else{
                        println!("\tElapsed: {:?}\n", end_time.duration_since(start_time).unwrap() );
                    }
                }
            }
        }
    } else {
        println!("Is an error fam");
    }
}

async fn handle_mark_bad(mut client: RawClient<HttpTransportClient>, block_cid: String) {
    if let Ok(cid) = Cid::from_raw_cid(block_cid.to_owned()){
        if let Ok(_) = SyncApi::mark_bad(&mut client, CidJson(cid)).await {
            println!("Successfully Marked block {} bad", block_cid);
        }
        else{
            println!("Failed to Mark block {} bad", block_cid);
        }
    }
    else {
        println!("Failed to decode input as cid")
    }
}

async fn handle_check_bad(mut client: RawClient<HttpTransportClient>, block_cid: String) {
    if let Ok(cid) = Cid::from_raw_cid(block_cid.to_owned()){
        let result = SyncApi::check_bad(&mut client, CidJson(cid)).await;
        if let Ok(msg) = result {
            println!("Successfully Checked block it is {} ", msg);
        }
        else{
            let err =  result.unwrap_err();
            println!("Failed to check block {} bad.\nThe error is {}", block_cid, err);
        }
    }
    else {
        println!("Failed to decode input as cid")
    }
}
