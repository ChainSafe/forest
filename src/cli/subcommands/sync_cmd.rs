// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::TipsetKey;
use crate::chain_sync::{ForkSyncInfo, NodeSyncStatus, SyncStage};
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
                let ticker = Ticker::new(0.., Duration::from_secs(2));
                let mut stdout = stdout();
                let mut last_lines_printed = 1;

                // if the sync stage is idle, check if the snapshot download is needed
                let initial_report = SyncStatusReport::call(&client, ()).await?;
                if initial_report.status == NodeSyncStatus::Initializing {
                    // Consider checking snapshot status if node is initializing
                    println!("Node initializing, checking snapshot status...");
                    if !check_snapshot_progress(&client, false)
                        .await?
                        .is_not_required()
                    {
                        println!("Snapshot download in progress, waiting...");
                        wait_for_snapshot_completion(&client).await?;
                        println!("Snapshot download complete. Starting sync monitor...");
                    } else {
                        println!("No snapshot download required or already complete.");
                    }
                }

                for _ in ticker {
                    let report = SyncStatusReport::call(&client, ()).await?;
                    if last_lines_printed > 0 {
                        write!(
                            stdout,
                            "\r{}{}",
                            anes::MoveCursorUp(last_lines_printed as u16),
                            anes::ClearBuffer::Below,
                        )?;
                    }
                    let mut current_lines = 0;
                    println!(
                        "Status: {:?} ({} epochs behind)",
                        report.status, report.epochs_behind
                    );
                    current_lines += 1;

                    let head_key_str = report
                        .current_head_key
                        .as_ref()
                        .map(|key| tipset_key_to_string(key))
                        .unwrap_or_else(|| "[unknown]".to_string());

                    println!(
                        "Node Head: Epoch {} ({})",
                        report.current_head_epoch, head_key_str
                    );

                    current_lines += 1;
                    println!("Network Head: Epoch {}", report.network_head_epoch);
                    current_lines += 1;
                    println!("Last Update: {}", report.last_updated);
                    current_lines += 1;

                    // Print active sync tasks (forks)
                    if report.active_forks.is_empty() {
                        println!("Active Sync Tasks: None");
                        current_lines += 1;
                    } else {
                        println!("Active Sync Tasks:");
                        current_lines += 1;
                        let mut sorted_forks = report.active_forks.clone();
                        sorted_forks.sort_by_key(|f| std::cmp::Reverse(f.target_epoch));
                        for fork in &report.active_forks {
                            print_fork_sync_info(fork, &mut current_lines)?;
                        }
                    }

                    last_lines_printed = current_lines;
                    // Break if Synced and not watching
                    if !watch && report.status == NodeSyncStatus::Synced {
                        // Perform one final clear and print before exiting
                        write!(
                            stdout,
                            "\r{}{}",
                            anes::MoveCursorUp(last_lines_printed as u16),
                            anes::ClearBuffer::Below,
                        )?;
                        println!(
                            "Status: {:?} ({} epochs behind)",
                            report.status, report.epochs_behind
                        );
                        println!(
                            "Node Head: Epoch {} ({})",
                            report.current_head_epoch, head_key_str
                        );
                        println!("Network Head: Epoch {}", report.network_head_epoch);
                        println!("Last Update: {}", report.last_updated.to_rfc3339());
                        let mut sorted_forks = report.active_forks.clone();
                        sorted_forks.sort_by_key(|f| std::cmp::Reverse(f.target_epoch));

                        if sorted_forks.is_empty() {
                            println!("Active Sync Tasks: None");
                        } else {
                            println!("Active Sync Tasks:");
                            for fork in &sorted_forks {
                                // Don't increment lines here, just print final state
                                print_fork_sync_info(fork, &mut 0)?;
                            }
                        }
                        println!("\nSync complete!");
                        break;
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

fn print_fork_sync_info(fork: &ForkSyncInfo, line_count: &mut usize) -> anyhow::Result<()> {
    let total_epochs_for_this_fork = fork
        .target_epoch
        .saturating_sub(fork.target_sync_epoch_start);
    println!(
        "  - Fork Target: {} ({}), Stage: {}, Syncing Range: [{}..{}] ({} epochs)",
        fork.target_epoch,
        tipset_key_to_string(&fork.target_tipset_key),
        &fork.stage,
        fork.target_sync_epoch_start,
        fork.target_epoch,
        total_epochs_for_this_fork
    );
    if *line_count > 0 {
        // Only increment if we are in the Wait command context
        *line_count += 1;
    }

    Ok(())
}

fn format_tipset_cids(cids: &str) -> &str {
    if cids.is_empty() { "[]" } else { cids }
}

fn tipset_key_to_string(key: &TipsetKey) -> String {
    if key.to_cids().is_empty() {
        "[]".to_string()
    } else {
        format!("[{}, ...]", key.to_cids().first())
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

        write!(
            stdout,
            "\r{}{}Snapshot status: {}",
            anes::MoveCursorUp(1),
            anes::ClearLine::All,
            progress_state
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
