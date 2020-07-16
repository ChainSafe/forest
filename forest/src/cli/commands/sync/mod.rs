use structopt::StructOpt;
use async_trait::async_trait;
use crate::{sub_cmd};
use cid::*;
use super::{CLICommand};

#[derive(StructOpt)]
pub struct SyncCommand{
    #[structopt(subcommand)]
    sub_cmd : SyncSubCmd
}

sub_cmd!(
    SyncSubCmd, "Subcommands", "Subcommands for inspecting or interacting with the chain syncer" =>
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
        match self.sub_cmd{
            SyncSubCmd::Status(_) => println!("In status sub command."),
            SyncSubCmd::Wait(_) => println!("In wait sub command."),
            SyncSubCmd::MarkBad(b) => println!("In mark-bad sub command. Block is {:?}", b.block_cid),
            SyncSubCmd::CheckBad(b) => println!("In check-bad sub command. Block is {:?}", b.block_cid),
        }
    }
}