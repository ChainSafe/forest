use structopt::StructOpt;
use async_trait::async_trait;
use crate::{sub_cmd};
use cid::*;
use super::{CLICommand};
use jsonrpsee;
use async_std;
use std::collections::HashMap;
use rpc::RPCSyncState;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use serde::{Serialize, Deserialize};

sub_cmd!(
    SyncCommand, "Subcommands", "Subcommands for inspecting or interacting with the chain syncer" =>
        Status, "status", "Check sync status",
        Wait, "wait", "Wait for sync to be complete",
        MarkBad, "mark-bad",  "Mark the given block as bad, will prevent syncing to a chain that contains it",
        CheckBad, "check-bad", "Check if the given block was marked bad, and for what reason",
);

#[derive(StructOpt)]
pub struct Status{}

#[derive(StructOpt)]
pub struct Wait{}

#[derive(StructOpt)]
pub struct MarkBad{
    #[structopt(short, long)]
    block_cid : String
}

#[derive(StructOpt)]
pub struct CheckBad{
    #[structopt(short, long)]
    block_cid : String
}

jsonrpsee::rpc_api! {
    
    SyncApi  {
        #[rpc(method = "Filecoin.SyncState")]
        fn status() -> RPCSyncState ;
    }
}

#[async_trait]
impl CLICommand for SyncCommand{
    async fn handle(self){
        match self {
            SyncCommand::Status(_) => {
                println!("In status sub command.");
                let transport_client = jsonrpsee::transport::http::HttpTransportClient::new("http://127.0.0.1:1234/rpc/v0");
                let mut client = jsonrpsee::raw::RawClient::new(transport_client);
                let v = async_std::task::block_on(async {
                    let v = SyncApi::status(&mut client).await;
                    if let Ok(r) = v{
                        println!("Is good yo {:?}", r);
                    }
                    else  {
                        println!("Is an error fam");
                    }

                });

                

                //println!("v is {:?}", v);
            },
            SyncCommand::Wait(_) => println!("In wait sub command."),
            SyncCommand::MarkBad(b) => println!("In mark-bad sub command. Block is {:?}", b.block_cid),
            SyncCommand::CheckBad(b) => println!("In check-bad sub command. Block is {:?}", b.block_cid),
        }
    }
}