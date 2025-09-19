// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::chain::FilecoinSnapshotVersion;
use crate::chain_sync::SyncConfig;
use crate::cli_shared::snapshot::{self, TrustedVendor};
use crate::db::car::forest::new_forest_car_temp_path_in;
use crate::networks::calibnet;
use crate::rpc::chain::ForestChainExportDiffParams;
use crate::rpc::{self, chain::ForestChainExportParams, prelude::*, types::ApiTipsetKey};
use anyhow::Context as _;
use chrono::DateTime;
use clap::Subcommand;
use indicatif::{ProgressBar, ProgressStyle};
use std::{
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
        /// How many state-roots to include. Lower limit is 900 for `calibnet` and `mainnet`.
        #[arg(short, long)]
        depth: Option<crate::chain::ChainEpochDelta>,
        /// Export snapshot in the experimental v2 format(FRC-0108).
        #[arg(long, value_enum, default_value_t = FilecoinSnapshotVersion::V1)]
        format: FilecoinSnapshotVersion,
    },
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
                let chain_head = ChainHead::call(&client, ()).await?;

                let epoch = tipset.unwrap_or(chain_head.epoch());

                let raw_network_name = StateNetworkName::call(&client, ()).await?;

                // For historical reasons and backwards compatibility if snapshot services or their
                // consumers relied on the `calibnet`, we use `calibnet` as the chain name.
                let chain_name = if raw_network_name == calibnet::NETWORK_GENESIS_NAME {
                    calibnet::NETWORK_COMMON_NAME
                } else {
                    raw_network_name.as_str()
                };

                let tipset =
                    ChainGetTipSetByHeight::call(&client, (epoch, Default::default())).await?;

                let output_path = match output_path.is_dir() {
                    true => output_path.join(snapshot::filename(
                        TrustedVendor::Forest,
                        chain_name,
                        DateTime::from_timestamp(tipset.min_ticket_block().timestamp as i64, 0)
                            .unwrap_or_default()
                            .naive_utc()
                            .date(),
                        epoch,
                        true,
                    )),
                    false => output_path.clone(),
                };

                let output_dir = output_path.parent().context("invalid output path")?;
                let temp_path = new_forest_car_temp_path_in(output_dir)?;

                let params = ForestChainExportParams {
                    version: format,
                    epoch,
                    recent_roots: depth.unwrap_or(SyncConfig::default().recent_state_roots),
                    output_path: temp_path.to_path_buf(),
                    tipset_keys: ApiTipsetKey(Some(chain_head.key().clone())),
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
                    if let Some(hash) = hash_result {
                        save_checksum(&output_path, hash).await?;
                    }
                    temp_path.persist(output_path)?;
                }

                println!("Export completed.");
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
