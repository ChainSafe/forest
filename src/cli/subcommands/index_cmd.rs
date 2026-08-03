// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::ipld::ChainExportState;
use crate::prelude::*;
use crate::rpc::{
    self,
    chain::{
        ApiIndexBackfillStatus, ChainHead, ChainValidateIndex, IndexBackfill, IndexBackfillCancel,
        IndexBackfillParams, IndexBackfillStatus,
    },
    prelude::*,
};
use clap::Subcommand;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::{Duration, Instant};

/// Manage the chain index
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
        #[arg(long)]
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
    /// validates the chain index entries for each epoch in descending order in the specified range, checking for missing or
    /// inconsistent entries (i.e. the indexed data does not match the actual chain state). If '--backfill' is enabled
    /// (which it is by default), it will attempt to backfill any missing entries using the `ChainValidateIndex` API.
    ValidateBackfill {
        /// specifies the starting tipset epoch for validation (inclusive)
        #[arg(long, required = true)]
        from: ChainEpoch,
        /// specifies the ending tipset epoch for validation (inclusive)
        #[arg(long, required = true)]
        to: ChainEpoch,
        /// determines whether to backfill missing index entries during validation
        #[arg(long, default_missing_value = "true", default_value = "true")]
        backfill: Option<bool>,
    },
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
            Self::ValidateBackfill { from, to, backfill } => {
                validate_backfill(&client, from, to, backfill.unwrap_or_default()).await
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
        ChainExportState::Succeeded => pb.finish_with_message(format!(
            "Backfill completed (indexed {}, skipped {})",
            last.indexed, last.skipped
        )),
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

async fn validate_backfill(
    client: &rpc::Client,
    from: ChainEpoch,
    to: ChainEpoch,
    backfill: bool,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        from > 0,
        "invalid from epoch: {from}, must be greater than 0"
    );
    anyhow::ensure!(to > 0, "invalid to epoch: {to}, must be greater than 0");
    anyhow::ensure!(
        to <= from,
        "to epoch ({to}) must be less than or equal to from epoch ({from})"
    );
    let head = ChainHead::call(client, ()).await?;
    anyhow::ensure!(
        from < head.epoch(),
        "from epoch ({from}) must be less than chain head ({})",
        head.epoch()
    );
    let start = Instant::now();
    tracing::info!(
        "starting chainindex validation; from epoch: {from}; to epoch: {to}; backfill: {backfill};"
    );
    let mut backfills = 0;
    let mut null_rounds = 0;
    let mut validations = 0;
    for epoch in (to..=from).rev() {
        match ChainValidateIndex::call(client, (epoch, backfill)).await {
            Ok(r) => {
                if r.backfilled {
                    backfills += 1;
                } else if r.is_null_round {
                    null_rounds += 1;
                } else {
                    validations += 1;
                }
            }
            Err(e) => {
                tracing::warn!("Failed to validate index at epoch {epoch}: {e}");
            }
        }
    }
    tracing::info!(
        "done with {backfills} backfills, {null_rounds} null rounds, {validations} validations, took {}",
        humantime::format_duration(start.elapsed())
    );
    Ok(())
}
