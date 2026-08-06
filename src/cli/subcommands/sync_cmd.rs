// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::TipsetKey;
use crate::chain_sync::{NodeSyncStatus, SyncStatusReport};
use crate::rpc::sync::{SnapshotProgressState, SyncStatus};
use crate::rpc::{self, prelude::*};
use anyhow::Context;
use cid::Cid;
use clap::Subcommand;
use dialoguer::console::{Term, measure_text_width};
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
                // Buffered so that the clear and the whole report reach the terminal
                // as a single write, leaving no partially redrawn frame on screen.
                let mut term = Term::buffered_stdout();
                let mut rows_printed_last_iteration = 0;

                handle_initial_snapshot_check(&client).await?;

                let mut interval = tokio::time::interval(Duration::from_secs(1));
                loop {
                    interval.tick().await;
                    let report = SyncStatus::call(&client, ())
                        .await
                        .context("Failed to get sync status")?;

                    wait_for_node_to_start_syncing(&client).await?;

                    clear_previous_lines(&term, rows_printed_last_iteration)?;

                    let width = terminal_width(&term);
                    rows_printed_last_iteration =
                        print_sync_report_details(&mut term, &report, width)
                            .context("Failed to print sync status report")?;
                    term.flush()?;

                    // Exit if synced and not in watch mode.
                    if !watch && report.status == NodeSyncStatus::Synced {
                        writeln!(term, "\nSync complete!")?;
                        term.flush()?;
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

                // Print the status report once, without row counting for clearing
                let mut term = Term::buffered_stdout();
                let width = terminal_width(&term);
                _ = print_sync_report_details(&mut term, &sync_status, width)
                    .context("Failed to print sync status report")?;
                term.flush()?;

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

/// Width of the terminal in columns, falling back to a sane default when it
/// cannot be determined (e.g. output is piped).
fn terminal_width(term: &Term) -> usize {
    term.size().1 as usize
}

/// Writes the sync status report and returns the number of terminal *rows* it
/// occupies.
///
/// Rows, not lines: a line wider than the terminal wraps onto several rows, and
/// [`clear_previous_lines`] moves the cursor by rows. Counting `writeln!` calls
/// instead leaves the topmost row of each frame behind on narrow terminals.
/// See <https://github.com/ChainSafe/forest/issues/7366>.
fn print_sync_report_details(
    out: &mut impl Write,
    report: &SyncStatusReport,
    term_width: usize,
) -> anyhow::Result<usize> {
    // Writes a single line and reports how many rows it takes up.
    let write_line = |out: &mut dyn Write, line: String| -> anyhow::Result<usize> {
        writeln!(out, "{line}")?;
        Ok(measure_text_width(&line).div_ceil(term_width).max(1))
    };

    let head_key_str = report
        .current_head_key
        .as_ref()
        .map(tipset_key_to_string)
        .unwrap_or_else(|| "[unknown]".to_string());

    let mut rows = 0;

    rows += write_line(
        out,
        format!(
            "Status: {:?} ({} epochs behind)",
            report.status, report.epochs_behind
        ),
    )?;
    rows += write_line(
        out,
        format!(
            "Node Head: Epoch {} ({head_key_str})",
            report.current_head_epoch
        ),
    )?;
    rows += write_line(
        out,
        format!("Network Head: Epoch {}", report.network_head_epoch),
    )?;
    rows += write_line(
        out,
        format!("Last Update: {}", report.last_updated.to_rfc3339()),
    )?;

    // Print active sync tasks (forks)
    let active_forks = &report.active_forks;
    if active_forks.is_empty() {
        rows += write_line(out, "Active Sync Tasks: None".into())?;
    } else {
        rows += write_line(out, "Active Sync Tasks:".into())?;
        let mut sorted_forks = active_forks.clone();
        sorted_forks.sort_by_key(|f| std::cmp::Reverse(f.target_epoch));
        for fork in &sorted_forks {
            let total_epochs_for_this_fork = fork
                .target_epoch
                .saturating_sub(fork.target_sync_epoch_start);
            rows += write_line(
                out,
                format!(
                    "  - Fork Target: {} ({}), Stage: {}, Syncing Range: [{}..{}] ({} epochs)",
                    fork.target_epoch,
                    tipset_key_to_string(&fork.target_tipset_key),
                    fork.stage,
                    fork.target_sync_epoch_start,
                    fork.target_epoch,
                    total_epochs_for_this_fork
                ),
            )?;
        }
    }

    Ok(rows)
}

/// Clears the `rows` terminal rows written by the previous refresh, leaving the
/// cursor at the start of the first cleared row so the next report overwrites it.
fn clear_previous_lines(term: &Term, rows: usize) -> anyhow::Result<()> {
    term.clear_last_lines(rows)?;
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
    let term = Term::stdout();
    const POLLING_INTERVAL: Duration = Duration::from_secs(1);

    loop {
        let report = SyncStatus::call(client, ())
            .await
            .context("Failed to get sync status while waiting for initialization to complete")?;

        if report.status == NodeSyncStatus::Initializing {
            term.write_str("\r🔄 Node syncing is initializing, please wait...")?;
            term.flush()?;
            is_msg_printed = true;

            sleep(POLLING_INTERVAL).await;
        } else {
            if is_msg_printed {
                term.clear_line()
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
