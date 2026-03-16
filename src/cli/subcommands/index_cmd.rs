// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    rpc::{
        self, RpcMethodExt as _,
        chain::{ChainHead, ChainValidateIndex},
    },
    shim::clock::ChainEpoch,
};
use clap::Subcommand;
use std::time::Instant;

/// Manage the chain index
#[derive(Debug, Subcommand)]
pub enum IndexCommands {
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
            Self::ValidateBackfill { from, to, backfill } => {
                validate_backfill(&client, from, to, backfill.unwrap_or_default()).await
            }
        }
    }
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
