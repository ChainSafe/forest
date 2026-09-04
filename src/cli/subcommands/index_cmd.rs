// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::ipld::ChainExportState;
use crate::rpc::chain::{
    ApiIndexBackfillStatus, IndexBackfill, IndexBackfillCancel, IndexBackfillParams,
    IndexBackfillStatus,
};
use crate::rpc::{self, prelude::*};
use crate::shim::clock::ChainEpoch;
use clap::Subcommand;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

#[derive(Debug, Subcommand)]
pub enum IndexCommands {
    /// Backfill the chain index (Ethereum mappings, events, block blooms) using the running node.
    ///
    /// Unlike `forest-tool index backfill`, this does not require the node to be stopped: the
    /// running daemon performs the backfill through its own database handle.
    #[command(group(clap::ArgGroup::new("range").required(true).args(["to", "n_tipsets"])))]
    Backfill {
        /// Starting tipset epoch for back-filling (inclusive). Defaults to the chain head, unless
        /// `--resume` is given and a resume checkpoint exists.
        #[arg(long)]
        from: Option<ChainEpoch>,
        /// Ending tipset epoch for back-filling (inclusive).
        #[arg(long)]
        to: Option<ChainEpoch>,
        /// Number of tipsets to back-fill.
        #[arg(long, conflicts_with = "to")]
        n_tipsets: Option<u64>,
        /// Recompute missing tipset state (expensive) instead of skipping it; tipsets that still
        /// can't be computed are skipped and reported rather than aborting the run.
        #[arg(long, num_args = 0..=1, default_value_t = true, default_missing_value = "true", action = clap::ArgAction::Set)]
        recompute: bool,
        /// Also index revert-prone tipsets newer than the EC-finalized epoch (up to the head). By
        /// default the walk is clamped to the EC-finalized epoch.
        #[arg(long)]
        allow_near_head: bool,
        /// Resume from the persisted checkpoint of a previous run instead of starting at the chain
        /// head. Ignored when `--from` is given.
        #[arg(long)]
        resume: bool,
        /// Trigger the backfill and return immediately without waiting for completion.
        #[arg(long)]
        no_wait: bool,
    },
    /// Show the status of the current (or last) index backfill.
    BackfillStatus {
        /// Wait until the backfill completes, showing progress.
        #[arg(long)]
        wait: bool,
    },
    /// Cancel the in-progress index backfill.
    BackfillCancel {},
}

impl IndexCommands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Backfill {
                from,
                to,
                n_tipsets,
                recompute,
                allow_near_head,
                resume,
                no_wait,
            } => {
                let params = IndexBackfillParams {
                    from,
                    to,
                    n_tipsets,
                    recompute,
                    allow_near_head,
                    resume,
                };
                client
                    .call(IndexBackfill::request((params,))?.with_timeout(Duration::from_secs(30)))
                    .await?;
                println!("Index backfill started.");
                if no_wait {
                    println!("Use `forest-cli index backfill-status` to monitor progress.");
                    return Ok(());
                }
                wait_for_backfill(&client).await
            }
            Self::BackfillStatus { wait } => {
                let status = client
                    .call(IndexBackfillStatus::request(())?.with_timeout(Duration::from_secs(30)))
                    .await?;
                if !wait || status.state != ChainExportState::Running {
                    println!("{status}");
                    return Ok(());
                }
                wait_for_backfill(&client).await
            }
            Self::BackfillCancel {} => {
                let cancelled = client
                    .call(IndexBackfillCancel::request(())?.with_timeout(Duration::from_secs(30)))
                    .await?;
                if cancelled {
                    println!("Index backfill cancelled.");
                } else {
                    println!("No index backfill in progress to cancel.");
                }
                Ok(())
            }
        }
    }
}

/// Polls `Forest.IndexBackfillStatus` until the backfill reaches a terminal state, rendering a
/// progress bar.
async fn wait_for_backfill(client: &rpc::Client) -> anyhow::Result<()> {
    let pb = ProgressBar::new(10000).with_message("Backfilling index");
    pb.set_style(
        ProgressStyle::with_template("[{elapsed_precise}] [{wide_bar}] {percent}% {msg}")
            .expect("indicatif template must be valid")
            .progress_chars("#>-"),
    );
    let last: ApiIndexBackfillStatus = loop {
        let status = client
            .call(IndexBackfillStatus::request(())?.with_timeout(Duration::from_secs(30)))
            .await?;
        let position = (status.progress.clamp(0.0, 1.0) * 10000.0).trunc() as u64;
        pb.set_position(position);
        if status.state != ChainExportState::Running {
            break status;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    match last.state {
        ChainExportState::Succeeded => {
            pb.finish_with_message(format!(
                "Backfill finished (indexed {}, skipped {})",
                last.indexed, last.skipped
            ));
            anyhow::ensure!(last.skipped == 0);
        }
        ChainExportState::Cancelled => pb.abandon_with_message(format!(
            "Backfill cancelled (indexed {}, skipped {})",
            last.indexed, last.skipped
        )),
        _ => {
            pb.abandon_with_message("Backfill failed");
            anyhow::bail!(
                "index backfill failed: {}",
                last.error.as_deref().unwrap_or("unknown error")
            );
        }
    }
    Ok(())
}
