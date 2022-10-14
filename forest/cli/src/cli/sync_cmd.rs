// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Config;
use cid::Cid;
use forest_chain_sync::SyncStage;
use forest_json::cid::CidJson;
use forest_rpc_client::*;
use std::{
    io::{stdout, Write},
    time::Duration,
};
use structopt::StructOpt;
use ticker::Ticker;

use crate::cli::{format_vec_pretty, handle_rpc_err};

#[derive(Debug, StructOpt)]
pub enum SyncCommands {
    /// Display continuous sync data until sync is complete
    Wait {
        /// Don't exit after node is synced
        #[structopt(short)]
        watch: bool,
    },
    /// Check sync status
    Status,
    /// Check if a given block is marked bad, and for what reason
    CheckBad {
        #[structopt(short)]
        /// The block CID to check
        cid: String,
    },
    /// Mark a given block as bad
    MarkBad {
        /// The block CID to mark as a bad block
        #[structopt(short)]
        cid: String,
    },
}

impl SyncCommands {
    pub async fn run(&self, config: Config) {
        match self {
            Self::Wait { watch } => {
                let watch = *watch;

                let ticker = Ticker::new(0.., Duration::from_secs(1));
                let mut stdout = stdout();

                for _ in ticker {
                    let response = sync_status((), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)
                        .unwrap();
                    let state = &response.active_syncs[0];

                    let target_height = if let Some(tipset) = state.target() {
                        tipset.epoch()
                    } else {
                        0
                    };

                    let base_height = if let Some(tipset) = state.base() {
                        tipset.epoch()
                    } else {
                        0
                    };

                    println!(
                        "Worker: 0; Base: {}; Target: {}; (diff: {})",
                        base_height,
                        target_height,
                        target_height - base_height
                    );
                    println!(
                        "State: {}; Current Epoch: {}; Todo: {}",
                        state.stage(),
                        base_height,
                        state.epoch()
                    );

                    for _ in 0..2 {
                        stdout.write_all("\r\x1b[2K\x1b[A".as_bytes()).unwrap();
                    }

                    if state.stage() == SyncStage::Complete && !watch {
                        println!("Done!");
                        break;
                    };
                }
            }
            Self::Status => {
                let response = sync_status((), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();

                let state = &response.active_syncs[0];
                let base = state.base();
                let elapsed_time = state.get_elapsed_time();
                let target = state.target();

                let (target_cids, target_height) = if let Some(tipset) = target {
                    let cid_vec = tipset.cids().iter().map(|cid| cid.to_string()).collect();
                    (format_vec_pretty(cid_vec), tipset.epoch())
                } else {
                    ("[]".to_string(), 0)
                };

                let (base_cids, base_height) = if let Some(tipset) = base {
                    let cid_vec = tipset.cids().iter().map(|cid| cid.to_string()).collect();
                    (format_vec_pretty(cid_vec), tipset.epoch())
                } else {
                    ("[]".to_string(), 0)
                };

                let height_diff = base_height - target_height;

                println!("sync status:");
                println!("Base:\t{}", base_cids);
                println!("Target:\t{} ({})", target_cids, target_height);
                println!("Height diff:\t{}", height_diff.abs());
                println!("Stage:\t{}", state.stage());
                println!("Height:\t{}", state.epoch());

                if let Some(duration) = elapsed_time {
                    println!("Elapsed time:\t{}s", duration.whole_seconds());
                }
            }
            Self::CheckBad { cid } => {
                let cid: Cid = cid.parse().unwrap();
                let response = sync_check_bad((CidJson(cid),), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)
                    .unwrap();

                if response.is_empty() {
                    println!("Block \"{}\" is not marked as a bad block", cid);
                } else {
                    println!("response");
                }
            }
            Self::MarkBad { cid } => {
                let cid: Cid = cid.parse().unwrap();
                match sync_mark_bad((CidJson(cid),), &config.client.rpc_token).await {
                    Ok(()) => println!("OK"),
                    Err(error) => handle_rpc_err(error),
                }
            }
        }
    }
}
