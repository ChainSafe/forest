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

                // Check if we should wait for snapshot to complete,
                // if the sync stage is idle, we should wait for snapshot to complete
                let should_wait_for_snapshot = SyncState::call(&client, ())
                    .await?
                    .active_syncs
                    .iter()
                    .any(|state| state.stage() == SyncStage::Idle);

                if should_wait_for_snapshot {
                    // Snapshot is not started, means node is not initialized yet, return
                    if wait_for_snapshot_completion(&client)
                        .await?
                        .eq(&SnapshotProgressState::NotStarted)
                    {
                        return Ok(());
                    }
                }

                'wait: for _ in ticker {
                    let resp = SyncState::call(&client, ()).await?;
                    let active_syncs = resp.active_syncs;

                    // Print status for all sync states
                    active_syncs.iter().for_each(|state| {
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
                    });

                    // Clear printed lines
                    (0..active_syncs.len() * 2).for_each(|_| {
                        write!(
                            stdout,
                            "\r{}{}",
                            anes::ClearLine::All,
                            anes::MoveCursorUp(1)
                        )
                        .expect("Failed to clear lines");
                    });

                    // Break if any state is Complete and we're not watching
                    if !watch
                        && active_syncs
                            .iter()
                            .any(|state| state.stage() == SyncStage::Complete)
                    {
                        println!("\nDone!");
                        break 'wait;
                    }
                }

                Ok(())
            }
            Self::Status => {
                let resp = client.call(SyncState::request(())?).await?;
                for state in resp.active_syncs {
                    let base = state.base();
                    let elapsed_time = state.get_elapsed_time();
                    let target = state.target();

                    let (target_cids, target_height) = if let Some(tipset) = target {
                        let cid_vec = tipset.cids().iter().map(|cid| cid.to_string()).collect();
                        (format_vec_pretty(cid_vec), tipset.epoch())
                    } else {
                        ("".to_string(), 0)
                    };

                    let (base_cids, base_height) = if let Some(tipset) = base {
                        let cid_vec = tipset.cids().iter().map(|cid| cid.to_string()).collect();
                        (format_vec_pretty(cid_vec), tipset.epoch())
                    } else {
                        ("".to_string(), 0)
                    };

                    let height_diff = base_height - target_height;

                    match state.stage() {
                        // If the sync state is idle, check if the snapshot is in progress once
                        SyncStage::Idle => _ = check_snapshot_progress(&client, false).await?,
                        _ => {
                            println!("sync status:");
                            println!("Base:\t{}", format_tipset_cids(&base_cids));
                            println!(
                                "Target:\t{} ({target_height})",
                                format_tipset_cids(&target_cids)
                            );
                            println!("Height diff:\t{}", height_diff.abs());
                            println!("Stage:\t{}", state.stage());
                            println!("Height:\t{}", state.epoch());

                            if let Some(duration) = elapsed_time {
                                println!("Elapsed time:\t{}s", duration.num_seconds());
                            }
                        }
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

fn format_tipset_cids(cids: &str) -> &str {
    if cids.is_empty() {
        "[]"
    } else {
        cids
    }
}

/// Check if the snapshot download is in progress, if wait is true,
/// wait till snapshot download is completed else return after checking once
async fn check_snapshot_progress(
    client: &rpc::Client,
    wait: bool,
) -> anyhow::Result<SnapshotProgressState> {
    let mut interval = time::interval(Duration::from_secs(5));
    let mut stdout = stdout();
    loop {
        interval.tick().await;
        let progress_state = client.call(SyncSnapshotProgress::request(())?).await?;
        match &progress_state {
            SnapshotProgressState::InProgress { message } => {
                // if wait is true, wait till snapshot download is completed
                match wait {
                    false => {
                        println!("🌳 Snapshot download in progress");
                        return Ok(progress_state);
                    }
                    _ => {
                        println!("🌳 Snapshot download in progress: {}", message);
                        write!(
                            stdout,
                            "\r{}{}",
                            anes::ClearLine::All,
                            anes::MoveCursorUp(1)
                        )?;
                        continue;
                    }
                }
            }
            SnapshotProgressState::Completed => {
                write!(
                    stdout,
                    "\r{}{}",
                    anes::ClearLine::All,
                    anes::MoveCursorUp(1)
                )?;
                println!("\n✅ Snapshot download completed! Chain will start syncing shortly");
            }
            SnapshotProgressState::NotStarted => {
                println!("⏳ Snapshot download not started - node might be initializing wait and retry in sometime")
            }
        }

        return Ok(progress_state);
    }
}

/// Wait for snapshot download to complete (convenience function)
async fn wait_for_snapshot_completion(
    client: &rpc::Client,
) -> anyhow::Result<SnapshotProgressState> {
    check_snapshot_progress(client, true).await
}
