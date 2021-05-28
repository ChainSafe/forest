// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::stringify_rpc_err;
use cid::Cid;
use rpc_client::{self, new_client};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub enum MpoolCommands {
    #[structopt(help = "Retrieve pending messages in mempool")]
    Pending {
        #[structopt(short, help = "a valid CID")]
        cid: String,
    },
}

impl MpoolCommands {
    pub async fn run(&self) {
        let mut client = new_client();
        match self {
            Self::Pending { cid } => {
                let cid: Cid = cid.parse().unwrap();
                let messages = rpc_client::pending(&mut client, cid)
                    .await
                    .map_err(stringify_rpc_err)
                    .unwrap();
                println!("{:#?}", messages);
            }
        }
    }
}
