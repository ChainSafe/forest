// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::chain_sync::SyncConfig;
use crate::cli_shared::snapshot::{self, TrustedVendor};
use crate::lotus_json::LotusJson;
use crate::rpc_api::chain_api::ChainExportParams;
use crate::rpc_api::data_types::ApiTipsetKey;
use crate::rpc_client::ApiInfo;
use anyhow::Context as _;
use chrono::DateTime;
use clap::Subcommand;
use human_repr::HumanCount;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
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
    },
}

impl SnapshotCommands {
    pub async fn run(self, api: ApiInfo) -> anyhow::Result<()> {
        match self {
            Self::Export {
                output_path,
                skip_checksum,
                dry_run,
                tipset,
                depth,
            } => {
                let chain_head = api.chain_head().await?;

                let epoch = tipset.unwrap_or(chain_head.epoch());

                let raw_network_name = api.state_network_name().await?;
                let chain_name = crate::daemon::get_actual_chain_name(&raw_network_name);

                let tipset = api
                    .chain_get_tipset_by_height(epoch, Default::default())
                    .await?;

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
                let temp_path = NamedTempFile::new_in(output_dir)?.into_temp_path();

                let params = ChainExportParams {
                    epoch,
                    recent_roots: depth.unwrap_or(SyncConfig::default().recent_state_roots),
                    output_path: temp_path.to_path_buf(),
                    tipset_keys: LotusJson(ApiTipsetKey(Some(chain_head.key().clone()))),
                    skip_checksum,
                    dry_run,
                };

                let handle = tokio::spawn({
                    let tmp_file = temp_path.to_owned();
                    let output_path = output_path.clone();
                    async move {
                        let mut interval =
                            tokio::time::interval(tokio::time::Duration::from_secs_f32(0.25));
                        println!("Getting ready to export...");
                        loop {
                            interval.tick().await;
                            let snapshot_size = std::fs::metadata(&tmp_file)
                                .map(|meta| meta.len())
                                .unwrap_or(0);
                            print!(
                                "{}{}",
                                anes::MoveCursorToPreviousLine(1),
                                anes::ClearLine::All
                            );
                            println!(
                                "{}: {}",
                                &output_path.to_string_lossy(),
                                snapshot_size.human_count_bytes()
                            );
                            let _ = std::io::stdout().flush();
                        }
                    }
                });

                let hash_result = api.chain_export(params).await?;

                handle.abort();
                let _ = handle.await;

                if let Some(hash) = hash_result {
                    save_checksum(&output_path, hash).await?;
                }
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
