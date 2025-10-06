// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::chain::FilecoinSnapshotVersion;
use crate::chain_sync::chain_muxer::DEFAULT_RECENT_STATE_ROOTS;
use crate::cli_shared::snapshot::{self, TrustedVendor};
use crate::db::car::forest::new_forest_car_temp_path_in;
use crate::networks::calibnet;
use crate::rpc::chain::ForestChainExportDiffParams;
use crate::rpc::types::ApiExportResult;
use crate::rpc::{self, chain::ForestChainExportParams, prelude::*};
use crate::shim::policy::policy_constants::CHAIN_FINALITY;
use anyhow::Context as _;
use chrono::DateTime;
use clap::Subcommand;
use indicatif::{ProgressBar, ProgressStyle};
use std::{
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::AsyncWriteExt;

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
        #[arg(long, value_enum, default_value_t = FilecoinSnapshotVersion::V1)]
        format: FilecoinSnapshotVersion,
    },
    /// Show status of the current export.
    ExportStatus {
        /// Wait until it completes and print progress.
        #[arg(long)]
        wait: bool,
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

                let output_path = match output_path.is_dir() {
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
                };

                let output_dir = output_path.parent().context("invalid output path")?;
                let temp_path = new_forest_car_temp_path_in(output_dir)?;

                let params = ForestChainExportParams {
                    version: format,
                    epoch: tipset.epoch(),
                    recent_roots: depth,
                    output_path: temp_path.to_path_buf(),
                    tipset_keys: tipset.key().clone().into(),
                    skip_checksum,
                    dry_run,
                };

                let pb = ProgressBar::new_spinner().with_style(
                    ProgressStyle::with_template(
                        "{spinner} {msg} {binary_total_bytes} written in {elapsed} ({binary_bytes_per_sec})",
                    )
                    .expect("indicatif template must be valid"),
                ).with_message(format!("Exporting {} ...", output_path.display()));
                pb.enable_steady_tick(std::time::Duration::from_millis(80));
                let handle = tokio::spawn({
                    let path: PathBuf = (&temp_path).into();
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
                let hash_result = client
                    .call(ForestChainExport::request((params,))?.with_timeout(Duration::MAX))
                    .await?;

                handle.abort();
                pb.finish();
                _ = handle.await;

                if !dry_run {
                    if let ApiExportResult::Done(Some(hash)) = hash_result.clone() {
                        save_checksum(&output_path, hash).await?;

                        temp_path.persist(output_path)?;
                    }
                }

                match hash_result {
                    ApiExportResult::Done(_) => {
                        println!("Export completed.");
                    }
                    ApiExportResult::Cancelled => {
                        println!("Export cancelled.");
                    }
                }
                Ok(())
            }
            Self::ExportStatus { wait } => {
                if wait {
                    let mut first = 0;
                    loop {
                        let result = client
                            .call(
                                ForestChainExportStatus::request(())?
                                    .with_timeout(Duration::from_secs(30)),
                            )
                            .await?;
                        if first == 0 && result.epoch != 0 {
                            first = result.epoch
                        }
                        //  1.0 - 3000 / 10000
                        print!(
                            "\r{}%",
                            ((1.0 - ((result.epoch as f64) / (first as f64))) * 100.0).trunc()
                        );

                        std::io::stdout().flush().unwrap();
                        if result.epoch == 0 {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    return Ok(());
                }
                let result = client
                    .call(ForestChainExportStatus::request(())?.with_timeout(Duration::MAX))
                    .await?;
                println!("{:?}", result);

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

                let output_path = match output_path.is_dir() {
                    true => output_path.join(format!(
                        "forest_snapshot_diff_{chain_name}_{from}_{to}+{depth}.car.zst"
                    )),
                    false => output_path.clone(),
                };

                let output_dir = output_path.parent().context("invalid output path")?;
                let temp_path = new_forest_car_temp_path_in(output_dir)?;

                let params = ForestChainExportDiffParams {
                    output_path: temp_path.to_path_buf(),
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
                let handle = tokio::spawn({
                    let path: PathBuf = (&temp_path).into();
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
                client
                    .call(ForestChainExportDiff::request((params,))?.with_timeout(Duration::MAX))
                    .await?;

                handle.abort();
                pb.finish();
                _ = handle.await;

                temp_path.persist(output_path)?;
                println!("Export completed.");
                Ok(())
            }
        }
    }
}

/// Prints hex-encoded representation of SHA-256 checksum and saves it to a file
/// with the same name but with a `.sha256sum` extension.
async fn save_checksum(source: &Path, encoded_hash: String) -> anyhow::Result<()> {
    let checksum_file_content = format!(
        "{encoded_hash} {}\n",
        source
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .context("Failed to retrieve file name while saving checksum")?
    );

    let checksum_path = PathBuf::from(source).with_extension("sha256sum");

    let mut checksum_file = tokio::fs::File::create(&checksum_path).await?;
    checksum_file
        .write_all(checksum_file_content.as_bytes())
        .await?;
    checksum_file.flush().await?;
    Ok(())
}
