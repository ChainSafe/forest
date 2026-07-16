// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::chain::FilecoinSnapshotVersion;
use crate::chain_sync::chain_muxer::DEFAULT_RECENT_STATE_ROOTS;
use crate::cli_shared::snapshot::{self, TrustedVendor};
use crate::db::car::forest::tmp_exporting_forest_car_path;
use crate::networks::calibnet;
use crate::prelude::*;
use crate::rpc::chain::ForestChainExportDiffParams;
use crate::rpc::types::ApiExportResult;
use crate::rpc::{self, chain::ForestChainExportParams, prelude::*};
use crate::shim::policy::policy_constants::CHAIN_FINALITY;
use chrono::DateTime;
use clap::Subcommand;
use indicatif::{ProgressBar, ProgressStyle};
use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum Format {
    Json,
    Text,
}

#[derive(Debug, Subcommand)]
pub enum SnapshotCommands {
    /// Export a snapshot of the chain to `<output_path>`
    Export {
        /// `./forest_snapshot_{chain}_{year}-{month}-{day}_height_{epoch}.car.zst`.
        #[arg(short, long, default_value = ".", verbatim_doc_comment)]
        output_path: PathBuf,
        /// Skip creating the checksum file.
        #[arg(long)]
        skip_checksum: bool,
        /// Don't write the archive.
        #[arg(long)]
        dry_run: bool,
        /// Tipset to start the export from, default is the chain head
        #[arg(short, long)]
        tipset: Option<i64>,
        /// How many state trees to include. 0 for chain spine with no state trees.
        #[arg(short, long, default_value_t = DEFAULT_RECENT_STATE_ROOTS)]
        depth: crate::chain::ChainEpochDelta,
        /// Snapshot format to export.
        #[arg(long, value_enum, default_value_t = FilecoinSnapshotVersion::V2)]
        format: FilecoinSnapshotVersion,
        /// Also exports an augmented data snapshot that contains message receipts and events
        #[arg(long)]
        augmented_data: bool,
        /// Also exports a tipset lookup HAMT snapshot
        #[arg(long)]
        tipset_lookup: bool,
    },
    /// Show status of the current export.
    ExportStatus {
        /// Wait until it completes and print progress.
        #[arg(long)]
        wait: bool,
        /// Format of the output. `json` or `text`.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Cancel the current export.
    ExportCancel {},
    /// Export a diff snapshot between `from` and `to` epochs to `<output_path>`
    ExportDiff {
        /// `./forest_snapshot_diff_{chain}_{from}_{to}+{depth}.car.zst`.
        #[arg(short, long, default_value = ".", verbatim_doc_comment)]
        output_path: PathBuf,
        /// Epoch to export from
        #[arg(long)]
        from: i64,
        /// Epoch to diff against
        #[arg(long)]
        to: i64,
        /// How many state-roots to include. Lower limit is 900 for `calibnet` and `mainnet`.
        #[arg(short, long)]
        depth: Option<crate::chain::ChainEpochDelta>,
    },
}

impl SnapshotCommands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Export {
                output_path,
                skip_checksum,
                dry_run,
                tipset,
                depth,
                format,
                augmented_data,
                tipset_lookup,
            } => {
                anyhow::ensure!(
                    depth >= 0,
                    "--depth must be non-negative; use 0 for spine-only snapshots"
                );

                if depth < CHAIN_FINALITY {
                    tracing::warn!(
                        "Depth {depth} should be no less than CHAIN_FINALITY {CHAIN_FINALITY} to export a valid lite snapshot"
                    );
                }

                let raw_network_name = StateNetworkName::call(&client, ()).await?;
                // For historical reasons and backwards compatibility if snapshot services or their
                // consumers relied on the `calibnet`, we use `calibnet` as the chain name.
                let chain_name = if raw_network_name == calibnet::NETWORK_GENESIS_NAME {
                    calibnet::NETWORK_COMMON_NAME
                } else {
                    raw_network_name.as_str()
                };

                let tipset = if let Some(epoch) = tipset {
                    // This could take a while when the requested epoch is far behind the chain head
                    client
                        .call(
                            ChainGetTipSetByHeight::request((epoch, Default::default()))?
                                .with_timeout(Duration::from_secs(60 * 15)),
                        )
                        .await?
                } else {
                    ChainHead::call(&client, ()).await?
                };

                let output_path = std::path::absolute(match output_path.is_dir() {
                    true => output_path.join(snapshot::filename(
                        TrustedVendor::Forest,
                        chain_name,
                        DateTime::from_timestamp(tipset.min_ticket_block().timestamp as i64, 0)
                            .unwrap_or_default()
                            .naive_utc()
                            .date(),
                        tipset.epoch(),
                        true,
                    )),
                    false => output_path.clone(),
                })
                .context("failed to make output path absolute")?;

                let params = ForestChainExportParams {
                    version: format,
                    epoch: tipset.epoch(),
                    recent_roots: depth,
                    output_path: output_path.clone(),
                    tipset_keys: tipset.key().clone().into(),
                    include_receipts: false,
                    include_events: false,
                    include_tipset_keys: false,
                    augmented_data,
                    tipset_lookup,
                    skip_checksum,
                    dry_run,
                };

                let pb = ProgressBar::new_spinner().with_style(
                    ProgressStyle::with_template(
                        "{spinner} {msg} {binary_total_bytes} written in {elapsed} ({binary_bytes_per_sec})",
                    )
                    .expect("indicatif template must be valid"),
                ).with_message(format!("Exporting v{} snapshot to {} ...", format as u64, output_path.display()));
                pb.enable_steady_tick(std::time::Duration::from_millis(80));
                let handle = tokio::spawn({
                    let path = tmp_exporting_forest_car_path(&output_path);
                    let pb = pb.clone();
                    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
                    async move {
                        loop {
                            interval.tick().await;
                            if let Ok(meta) = std::fs::metadata(&path) {
                                pb.set_position(meta.len());
                            }
                        }
                    }
                });

                // Manually construct RpcRequest because snapshot export could
                // take a few hours on mainnet
                let export_result = client
                    .call(ForestChainExport::request((params,))?.with_timeout(Duration::MAX))
                    .await?;

                handle.abort();
                pb.finish();
                _ = handle.await;

                match export_result {
                    ApiExportResult::Done => {
                        println!("Export completed.");
                    }
                    ApiExportResult::Cancelled => {
                        println!("Export cancelled.");
                    }
                }
                Ok(())
            }
            Self::ExportStatus { wait, format } => {
                let result = client
                    .call(
                        ForestChainExportStatus::request(())?.with_timeout(Duration::from_secs(30)),
                    )
                    .await?;
                if !result.exporting
                    && let Format::Text = format
                {
                    if result.cancelled {
                        println!("No export in progress (last export was cancelled)");
                    } else {
                        println!("No export in progress");
                    }
                    return Ok(());
                }
                if wait {
                    let elapsed = chrono::Utc::now()
                        .signed_duration_since(result.start_time.unwrap_or_default())
                        .to_std()
                        .unwrap_or(Duration::ZERO);
                    let pb = ProgressBar::new(10000)
                        .with_elapsed(elapsed)
                        .with_message("Exporting");
                    pb.set_style(
                        ProgressStyle::with_template(
                            "[{elapsed_precise}] [{wide_bar}] {percent}% {msg} ",
                        )
                        .expect("indicatif template must be valid")
                        .progress_chars("#>-"),
                    );
                    loop {
                        let result = client
                            .call(
                                ForestChainExportStatus::request(())?
                                    .with_timeout(Duration::from_secs(30)),
                            )
                            .await?;
                        if result.cancelled {
                            pb.set_message("Export cancelled");
                            pb.abandon();
                            return Ok(());
                        }
                        let position = (result.progress.clamp(0.0, 1.0) * 10000.0).trunc() as u64;
                        pb.set_position(position);

                        if !result.exporting {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }

                    pb.finish_with_message(if result.succeeded {
                        "Export completed"
                    } else {
                        "Export failed"
                    });

                    return Ok(());
                }
                match format {
                    Format::Text => {
                        println!("Exporting: {:.1}%", result.progress.clamp(0.0, 1.0) * 100.0);
                    }
                    Format::Json => {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    }
                }

                Ok(())
            }
            Self::ExportCancel {} => {
                let result = client
                    .call(
                        ForestChainExportCancel::request(())?.with_timeout(Duration::from_secs(30)),
                    )
                    .await?;
                if result {
                    println!("Export cancelled.");
                } else {
                    println!("No export in progress to cancel.");
                }
                Ok(())
            }
            Self::ExportDiff {
                output_path,
                from,
                to,
                depth,
            } => {
                let raw_network_name = StateNetworkName::call(&client, ()).await?;

                // For historical reasons and backwards compatibility if snapshot services or their
                // consumers relied on the `calibnet`, we use `calibnet` as the chain name.
                let chain_name = if raw_network_name == calibnet::NETWORK_GENESIS_NAME {
                    calibnet::NETWORK_COMMON_NAME
                } else {
                    raw_network_name.as_str()
                };

                let depth = depth.unwrap_or_else(|| from - to);
                anyhow::ensure!(depth > 0, "depth must be positive");

                let output_path = std::path::absolute(match output_path.is_dir() {
                    true => output_path.join(format!(
                        "forest_snapshot_diff_{chain_name}_{from}_{to}+{depth}.car.zst"
                    )),
                    false => output_path.clone(),
                })
                .context("failed to make output path absolute")?;

                let params = ForestChainExportDiffParams {
                    output_path: output_path.clone(),
                    from,
                    to,
                    depth,
                };

                let pb = ProgressBar::new_spinner().with_style(
                    ProgressStyle::with_template(
                        "{spinner} {msg} {binary_total_bytes} written in {elapsed} ({binary_bytes_per_sec})",
                    )
                    .expect("indicatif template must be valid"),
                ).with_message(format!("Exporting {} ...", output_path.display()));
                pb.enable_steady_tick(std::time::Duration::from_millis(80));
                let cancellation_token = CancellationToken::new();
                // Make sure token is cancelled on error path
                let _cancellation_token_drop_guard = cancellation_token.drop_guard_ref();
                let handle = tokio::spawn({
                    let cancellation_token = cancellation_token.clone();
                    let path = tmp_exporting_forest_car_path(&output_path);
                    let pb = pb.clone();
                    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
                    async move {
                        while !cancellation_token.is_cancelled() {
                            interval.tick().await;
                            if let Ok(meta) = std::fs::metadata(&path) {
                                pb.set_position(meta.len());
                            }
                        }
                    }
                });
                // Manually construct RpcRequest because snapshot export could
                // take a few hours on mainnet
                let export_result = client
                    .call(ForestChainExportDiff::request((params,))?.with_timeout(Duration::MAX))
                    .await?;
                // cancel before `handle.await` to avoid deadlock
                cancellation_token.cancel();
                pb.finish();
                _ = handle.await;

                match export_result {
                    ApiExportResult::Done => {
                        println!("Export completed.");
                    }
                    ApiExportResult::Cancelled => {
                        println!("Export cancelled.");
                    }
                }
                Ok(())
            }
        }
    }
}
