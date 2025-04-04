// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::chain_sync::SyncStage;
use crate::cli::subcommands::format_vec_pretty;
use crate::rpc::sync::SnapshotProgressState;
use crate::rpc::{self, prelude::*};
use cid::Cid;
use clap::Subcommand;
use std::{
    io::{Write, stdout},
    time::Duration,
};
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

                // if the sync stage is idle, check if the snapshot download is needed
                let check_snapshot_status = SyncState::call(&client, ())
                    .await?
                    .active_syncs
                    .iter()
                    .any(|state| state.stage() == SyncStage::Idle);

                // Check if we should wait for snapshot to complete,
                if check_snapshot_status {
                    println!("Checking snapshot status");
                    wait_for_snapshot_completion(&client).await?;
                    println!("Start syncing");
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
                        // If the sync state is idle, check the snapshot state once
                        SyncStage::Idle => {
                            if !check_snapshot_progress(&client, false)
                                .await?
                                .is_not_required()
                            {
                                continue;
                            }
                        }
                        _ => {}
                    }

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
    if cids.is_empty() { "[]" } else { cids }
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

        // Update the previous line
        write!(
            stdout,
            "\r{}{}Snapshot status: {}",
            anes::MoveCursorUp(1),
            anes::ClearLine::All,
            format!("{progress_state}")
        )?;
        stdout.flush()?;

        match progress_state {
            SnapshotProgressState::Completed | SnapshotProgressState::NotRequired => {
                println!();
                return Ok(progress_state);
            }
            _ if !wait => {
                println!();
                return Ok(progress_state);
            }
            _ => {} // continue
        }
    }
}

/// Wait for snapshot download to complete (convenience function)
async fn wait_for_snapshot_completion(
    client: &rpc::Client,
) -> anyhow::Result<SnapshotProgressState> {
    check_snapshot_progress(client, true).await
}
