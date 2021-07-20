// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use structopt::StructOpt;

use cid::json::vec::CidJsonVec;
use rpc_client::mpool_ops::*;

use crate::cli::handle_rpc_err;

#[derive(Debug, StructOpt)]
pub enum MpoolCommands {
    #[structopt(help = "Get pending messages")]
    Pending,
    #[structopt(help = "Print mempool stats")]
    Stat,
    #[structopt(help = "Subscribe to mempool changes")]
    Subscribe,
}

impl MpoolCommands {
    pub async fn run(&self) {
        match self {
            Self::Pending => {
                let res = mpool_pending((CidJsonVec(vec![]),)).await;
                let messages = res.map_err(handle_rpc_err).unwrap();
                println!("{:#?}", messages);
            }
            Self::Stat => {}
            Self::Subscribe => {}
        }
    }
}
