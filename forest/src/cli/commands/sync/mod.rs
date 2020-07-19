use structopt::StructOpt;
use async_trait::async_trait;
use crate::{sub_cmd};
use cid::*;
use super::{CLICommand};

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
     

#[async_trait]
impl CLICommand for SyncCommand{
    async fn handle(self){
        match self {
            SyncCommand::Status(_) => println!("In status sub command."),
            SyncCommand::Wait(_) => println!("In wait sub command."),
            SyncCommand::MarkBad(b) => println!("In mark-bad sub command. Block is {:?}", b.block_cid),
            SyncCommand::CheckBad(b) => println!("In check-bad sub command. Block is {:?}", b.block_cid),
        }
    }
}