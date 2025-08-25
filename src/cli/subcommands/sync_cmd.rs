// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::TipsetKey;
use crate::chain_sync::{ForkSyncInfo, NodeSyncStatus, SyncStatusReport};
use crate::rpc::sync::{SnapshotProgressState, SyncStatus};
use crate::rpc::{self, prelude::*};
use anyhow::Context;
use cid::Cid;
use clap::Subcommand;
use std::{
    io::{Write, stdout},
    time::Duration,
};
use tokio::time;
use tokio::time::sleep;

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
                let mut stdout = stdout();
                let mut lines_printed_last_iteration = 0;

                handle_initial_snapshot_check(&client).await?;

                let mut interval = tokio::time::interval(Duration::from_secs(1));
                loop {
                    interval.tick().await;
                    let report = SyncStatus::call(&client, ())
                        .await
                        .context("Failed to get sync status")?;

                    wait_for_node_to_start_syncing(&client).await?;

                    clear_previous_lines(&mut stdout, lines_printed_last_iteration)?;

                    lines_printed_last_iteration = print_sync_report_details(&report)
                        .context("Failed to print sync status report")?;

                    // Exit if synced and not in watch mode.
                    if !watch && report.status == NodeSyncStatus::Synced {
                        println!("\nSync complete!");
                        break;
                    }
                }

                Ok(())
            }

            Self::Status => {
                let sync_status = client.call(SyncStatus::request(())?).await?;
                if sync_status.status == NodeSyncStatus::Initializing {
                    // If a snapshot is required and not yet complete, return here
                    if !check_snapshot_progress(&client, false)
                        .await?
                        .is_not_required()
                    {
                        println!("Please try again later, once the snapshot is downloaded...");
                        return Ok(());
                    };
                }

                // Print the status report once, without line counting for clearing
                _ = print_sync_report_details(&sync_status)
                    .context("Failed to print sync status report")?;

                Ok(())
            }
            Self::CheckBad { cid } => {
                let response = SyncCheckBad::call(&client, (cid,)).await?;
                if response.is_empty() {
                    println!("Block \"{cid}\" is not marked as a bad block");
                } else {
                    println!("{response}");
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

/// Prints the sync status report details and returns the number of lines printed.
fn print_sync_report_details(report: &SyncStatusReport) -> anyhow::Result<usize> {
    let mut lines_printed_count = 0;

    println!(
        "Status: {:?} ({} epochs behind)",
        report.status, report.epochs_behind
    );
    lines_printed_count += 1;

    let head_key_str = report
        .current_head_key
        .as_ref()
        .map(tipset_key_to_string)
        .unwrap_or_else(|| "[unknown]".to_string());
    println!(
        "Node Head: Epoch {} ({})",
        report.current_head_epoch, head_key_str
    );
    lines_printed_count += 1;

    println!("Network Head: Epoch {}", report.network_head_epoch);
    lines_printed_count += 1;

    println!("Last Update: {}", report.last_updated.to_rfc3339());
    lines_printed_count += 1;

    // Print active sync tasks (forks)
    let active_forks = &report.active_forks;
    if active_forks.is_empty() {
        println!("Active Sync Tasks: None");
        lines_printed_count += 1;
    } else {
        println!("Active Sync Tasks:");
        lines_printed_count += 1;
        let mut sorted_forks = active_forks.clone();
        sorted_forks.sort_by_key(|f| std::cmp::Reverse(f.target_epoch));
        for fork in &sorted_forks {
            // Assuming print_fork_sync_info exists and increments line_count internally if needed
            // If print_fork_sync_info doesn't increment, adjust line_count here.
            // For simplicity, assuming it behaves as needed or is adjusted elsewhere.
            lines_printed_count += print_fork_sync_info(fork)?;
        }
    }

    Ok(lines_printed_count)
}

/// Prints fork sync info and returns the number of lines printed (expected to be 1).
fn print_fork_sync_info(fork: &ForkSyncInfo) -> anyhow::Result<usize> {
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
    Ok(1)
}

fn clear_previous_lines(stdout: &mut std::io::Stdout, lines: usize) -> anyhow::Result<()> {
    if lines > 0 {
        // Move cursor up `lines` times, return to start (\r), clear below
        write!(
            stdout,
            "\r{}{}",
            anes::MoveCursorUp(lines as u16),
            anes::ClearBuffer::Below,
        )?;
    }
    Ok(())
}

fn tipset_key_to_string(key: &TipsetKey) -> String {
    let cids = key.to_cids();
    match cids.len() {
        0 => "[]".to_string(),
        _ => format!("[{}, ...]", cids.first()),
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
            "\r{}{}Snapshot status: {}\n",
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
                return Ok(progress_state);
            }
            _ => {} // continue
        }
    }
}

/// Waits for node initialization to complete (start `Syncing`).
async fn wait_for_node_to_start_syncing(client: &rpc::Client) -> anyhow::Result<()> {
    let mut is_msg_printed = false;
    let mut stdout = stdout();
    const POLLING_INTERVAL: Duration = Duration::from_secs(1);

    loop {
        let report = SyncStatus::call(client, ())
            .await
            .context("Failed to get sync status while waiting for initialization to complete")?;

        if report.status == NodeSyncStatus::Initializing {
            write!(stdout, "\r🔄 Node syncing is initializing, please wait...")?;
            stdout.flush()?;
            is_msg_printed = true;

            sleep(POLLING_INTERVAL).await;
        } else {
            if is_msg_printed {
                clear_previous_lines(&mut stdout, 1)
                    .context("Failed to clear initializing message")?;
            }

            break;
        }
    }

    Ok(())
}

/// Checks if a snapshot download is required or in progress when the node is initializing.
/// If a snapshot download is in progress, it waits for completion before starting the sync monitor.
async fn handle_initial_snapshot_check(client: &rpc::Client) -> anyhow::Result<()> {
    let initial_report = SyncStatus::call(client, ())
        .await
        .context("Failed to get sync status")?;
    if initial_report.status == NodeSyncStatus::Initializing {
        // if the snapshot download is not required, then return,
        // else wait till the snapshot download is completed.
        if !check_snapshot_progress(client, false)
            .await?
            .is_not_required()
        {
            check_snapshot_progress(client, true).await?;
        }
    }

    Ok(())
}
