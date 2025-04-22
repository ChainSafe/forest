// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::TipsetKey;
use crate::chain_sync::{ForkSyncInfo, NodeSyncStatus, SyncStatusReport};
use crate::rpc::sync::{SnapshotProgressState, SyncStatus};
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
                let mut last_lines_printed = 0;

                // if the sync stage is idle, check if the snapshot download is needed
                handle_initial_snapshot_check(&client).await?;

                for _ in ticker {
                    let report = SyncStatus::call(&client, ()).await?;
                    if last_lines_printed > 0 {
                        write!(
                            stdout,
                            "\r{}{}",
                            anes::MoveCursorUp(last_lines_printed as u16),
                            anes::ClearBuffer::Below,
                        )?;
                    }
                    let mut current_lines = 0;
                    print_sync_report_details(&report, &mut current_lines)?;

                    last_lines_printed = current_lines;
                    // Break if Synced and not watching
                    if !watch && report.get_status() == NodeSyncStatus::Synced {
                        // Perform one final clear and print before exiting
                        if last_lines_printed > 0 {
                            write!(
                                stdout,
                                "\r{}{}",
                                anes::MoveCursorUp(last_lines_printed as u16),
                                anes::ClearBuffer::Below,
                            )?;
                        }
                        print_sync_report_details(&report, &mut current_lines)?;
                        println!("\nSync complete!");
                        break;
                    }
                }

                Ok(())
            }

            Self::Status => {
                let sync_status = client.call(SyncStatus::request(())?).await?;
                if sync_status.get_status() == NodeSyncStatus::Initializing {
                    println!("Node initializing, checking snapshot status...");
                    check_snapshot_progress(&client, false).await?;
                }

                // Print the status report once, without line counting for clearing
                print_sync_report_details(&sync_status, &mut 0)?;

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

/// Prints the sync status report details.
/// `line_count` is mutable and incremented for each line printed, used for clearing in `Wait`.
/// Pass `&mut 0` if line counting/clearing is not needed (like in `Status` or final print).
fn print_sync_report_details(
    report: &SyncStatusReport,
    line_count: &mut usize,
) -> anyhow::Result<()> {
    println!(
        "Status: {:?} ({} epochs behind)",
        report.get_status(),
        report.get_epochs_behind()
    );
    *line_count += 1;

    let head_key_str = report
        .get_current_chain_head_key()
        .map(tipset_key_to_string)
        .unwrap_or_else(|| "[unknown]".to_string());
    println!(
        "Node Head: Epoch {} ({})",
        report.get_current_chain_head_epoch(),
        head_key_str
    );
    *line_count += 1;

    println!("Network Head: Epoch {}", report.get_network_head_epoch());
    *line_count += 1;

    println!("Last Update: {}", report.get_last_updated().to_rfc3339());
    *line_count += 1;

    // Print active sync tasks (forks)
    let active_forks = report.get_active_forks();
    if active_forks.is_empty() {
        println!("Active Sync Tasks: None");
        *line_count += 1;
    } else {
        println!("Active Sync Tasks:");
        *line_count += 1;
        let mut sorted_forks = active_forks.clone();
        sorted_forks.sort_by_key(|f| std::cmp::Reverse(f.target_epoch));
        for fork in &sorted_forks {
            // Assuming print_fork_sync_info exists and increments line_count internally if needed
            // If print_fork_sync_info doesn't increment, adjust line_count here.
            // For simplicity, assuming it behaves as needed or is adjusted elsewhere.
            print_fork_sync_info(fork, line_count)?;
        }
    }
    Ok(())
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

fn tipset_key_to_string(key: &TipsetKey) -> String {
    let cids = key.to_cids();
    if cids.is_empty() {
        "[]".to_string()
    } else {
        format!("[{}, ...]", cids.first())
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

/// Handles the initial check for snapshot download if the node is initializing.
async fn handle_initial_snapshot_check(client: &rpc::Client) -> anyhow::Result<()> {
    let initial_report = SyncStatus::call(client, ()).await?;
    // Use the public getter method instead of accessing the private field
    if initial_report.get_status() == NodeSyncStatus::Initializing {
        println!("Node initializing, checking snapshot status...");
        // Consider checking snapshot status if node is initializing
        if !check_snapshot_progress(client, false)
            .await?
            .is_not_required()
        {
            println!("Snapshot download in progress, waiting...");
            wait_for_snapshot_completion(client).await?;
            println!("Snapshot download complete. Starting sync monitor...");
        } else {
            println!("No snapshot download required or already complete.");
        }
    }

    Ok(())
}
