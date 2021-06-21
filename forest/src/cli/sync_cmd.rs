// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use structopt::StructOpt;

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
            Self::Status => {}
            Self::CheckBad { cid } => {}
            Self::MarkBad { cid } => {}
        }
    }
}
