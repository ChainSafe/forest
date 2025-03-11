// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    io::{stdout, Write},
    time::Duration,
};

use crate::chain_sync::SyncStage;
use crate::cli::subcommands::format_vec_pretty;
use crate::rpc::sync::SnapshotProgressState;
use crate::rpc::{self, prelude::*};
use cid::Cid;
use clap::Subcommand;
use itertools::Itertools as _;
use ticker::Ticker;
use tokio::time;

#[derive(Debug, Subcommand)]
pub enum SyncCommands {
    /// Display continuous sync data until sync is complete
    Wait {
        /// Don't exit after node is synced
        #[arg(short)]
        watch: bool,
    },
    /// Check sync status
    Status,
    /// Check if a given block is marked bad, and for what reason
    CheckBad {
        #[arg(short)]
        /// The block CID to check
        cid: Cid,
    },
    /// Mark a given block as bad
    MarkBad {
        /// The block CID to mark as a bad block
        #[arg(short)]
        cid: Cid,
    },
}

impl SyncCommands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Wait { watch } => {
                let ticker = Ticker::new(0.., Duration::from_secs(1));
                let mut stdout = stdout();

                for _ in ticker {
                    let resp = SyncState::call(&client, ()).await?;
                    let active_syncs = resp.active_syncs;
                    let state = active_syncs
                        .iter()
                        .rev()
                        .find_or_first(|ss| {
                            ss.stage() != SyncStage::Idle && ss.stage() != SyncStage::Complete
                        })
                        .expect("Infallible, active_syncs is NonEmpty");

                    let base_height = state
                        .base()
                        .as_ref()
                        .map(|ts| ts.epoch())
                        .unwrap_or_default();
                    let target_height = state
                        .target()
                        .as_ref()
                        .map(|ts| ts.epoch())
                        .unwrap_or_default();

                    println!(
                        "Worker: 0; Base: {}; Target: {}; (diff: {})",
                        base_height,
                        target_height,
                        target_height - base_height
                    );
                    println!(
                        "State: {}; Current Epoch: {}; Todo: {}",
                        state.stage(),
                        state.epoch(),
                        target_height - state.epoch()
                    );

                    for _ in 0..2 {
                        write!(
                            stdout,
                            "\r{}{}",
                            anes::ClearLine::All,
                            anes::MoveCursorUp(1)
                        )?;
                    }

                    if state.stage() == SyncStage::Complete && !watch {
                        println!("\nDone!");
                        break;
                    };
                }

                Ok(())
            }
            Self::Status => {
                let resp = client.call(SyncState::request(())?).await?;
                let state = resp.active_syncs.first();

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

                // If the sync state is not in the Complete stage and both base and target tipsets are empty,
                // the node might be downloading the snapshot.
                if state.stage() != SyncStage::Complete
                    && base_cids.is_empty()
                    && target_cids.is_empty()
                {
                    check_snapshot_progress(&client).await?;
                } else {
                    println!("sync status:");
                    println!("Base:\t{base_cids}");
                    println!("Target:\t{target_cids} ({target_height})");
                    println!("Height diff:\t{}", height_diff.abs());
                    println!("Stage:\t{}", state.stage());
                    println!("Height:\t{}", state.epoch());

                    if let Some(duration) = elapsed_time {
                        println!("Elapsed time:\t{}s", duration.num_seconds());
                    }
                }

                Ok(())
            }
            Self::CheckBad { cid } => {
                let response = SyncCheckBad::call(&client, (cid,)).await?;
                if response.is_empty() {
                    println!("Block \"{cid}\" is not marked as a bad block");
                } else {
                    println!("response");
                }
                Ok(())
            }
            Self::MarkBad { cid } => {
                SyncMarkBad::call(&client, (cid,)).await?;
                println!("OK");
                Ok(())
            }
        }
    }
}

/// Check if the snapshot download is in progress, if it is then wait till the snapshot download is done
async fn check_snapshot_progress(client: &rpc::Client) -> anyhow::Result<()> {
    let mut interval = time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        let progress_state = client.call(SyncSnapshotProgress::request(())?).await?;
        match progress_state {
            SnapshotProgressState::InProgress { message } => {
                print!(
                    "\r{}{}🌳 Snapshot download in progress: {}",
                    anes::ClearLine::All,
                    anes::MoveCursorUp(1),
                    message
                );
                stdout().flush()?;
                continue;
            }
            SnapshotProgressState::Completed => {
                println!("\n✅ Snapshot download completed! Chain will start syncing shortly (retry sync status command in 5 seconds)...");
            }
            SnapshotProgressState::NotStarted => {
                println!("⏳ Snapshot download not started - node is initializing")
            }
        }

        return Ok(());
    }
}
