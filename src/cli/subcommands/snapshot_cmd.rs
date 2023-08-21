// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::cli::subcommands::{cli_error_and_die, handle_rpc_err};
use crate::cli_shared::snapshot::{self, TrustedVendor};
use crate::rpc_api::chain_api::ChainExportParams;
use crate::rpc_client::chain_ops::*;
use crate::utils::bail_moved_cmd;
use crate::utils::db::car_stream::CarStream;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::Subcommand;
use dialoguer::{theme::ColorfulTheme, Confirm};
use futures::TryStreamExt;
use human_repr::HumanCount;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use tokio::fs::File;
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

    // This subcommand is hidden and only here to help users migrating to forest-tool
    #[command(hide = true)]
    Fetch {
        #[arg(short, long, default_value = ".")]
        directory: PathBuf,
        #[arg(short, long, value_enum, default_value_t = snapshot::TrustedVendor::default())]
        vendor: snapshot::TrustedVendor,
    },

    // This subcommand is hidden and only here to help users migrating to forest-tool
    #[command(hide = true)]
    Validate {
        #[arg(long, default_value_t = 2000)]
        check_links: u32,
        #[arg(long)]
        check_network: Option<crate::networks::NetworkChain>,
        #[arg(long, default_value_t = 60)]
        check_stateroots: u32,
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
    },

    /// Make this snapshot suitable for use as a compressed car-backed blockstore.
    Compress {
        /// Input CAR file, in `.car`, `.car.zst`, or `.forest.car.zst` format.
        source: PathBuf,
        /// Output file, will be in `.forest.car.zst` format.
        ///
        /// Will reuse the source name (with new extension) if pointed to a
        /// directory.
        #[arg(short, long, default_value = ".")]
        output_path: PathBuf,
        #[arg(long, default_value_t = 3)]
        compression_level: u16,
        /// End zstd frames after they exceed this length
        #[arg(long, default_value_t = 8000usize.next_power_of_two())]
        frame_size: usize,
        /// Overwrite output file without prompting.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
}

impl SnapshotCommands {
    pub async fn run(self, config: Config) -> Result<()> {
        match self {
            Self::Export {
                output_path,
                skip_checksum,
                dry_run,
                tipset,
                depth,
            } => {
                let chain_head = match chain_head(&config.client.rpc_token).await {
                    Ok(LotusJson(head)) => head,
                    Err(_) => cli_error_and_die("Could not get network head", 1),
                };

                let epoch = tipset.unwrap_or(chain_head.epoch());

                let chain_name = chain_get_name((), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                let output_path = match output_path.is_dir() {
                    true => output_path.join(snapshot::filename(
                        TrustedVendor::Forest,
                        chain_name,
                        Utc::now().date_naive(),
                        epoch,
                        true,
                    )),
                    false => output_path.clone(),
                };

                let output_dir = output_path.parent().context("invalid output path")?;
                let temp_path = NamedTempFile::new_in(output_dir)?.into_temp_path();

                let params = ChainExportParams {
                    epoch,
                    recent_roots: depth.unwrap_or(config.chain.recent_state_roots),
                    output_path: temp_path.to_path_buf(),
                    tipset_keys: chain_head.key().clone(),
                    skip_checksum,
                    dry_run,
                };

                let finality = config.chain.policy.chain_finality.min(epoch);
                if params.recent_roots < finality {
                    bail!(
                        "For {}, depth has to be at least {finality}.",
                        config.chain.network
                    );
                }

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

                let hash_result = chain_export(params, &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                handle.abort();
                let _ = handle.await;

                if let Some(hash) = hash_result {
                    save_checksum(&output_path, hash).await?;
                }
                temp_path.persist(output_path)?;

                println!("Export completed.");
                Ok(())
            }
            Self::Fetch { .. } => bail_moved_cmd("snapshot fetch", "forest-tool snapshot fetch"),
            Self::Validate { .. } => {
                bail_moved_cmd("snapshot validate", "forest-tool snapshot validate")
            }
            Self::Compress {
                source,
                output_path,
                compression_level,
                frame_size,
                force,
            } => {
                // If input is 'snapshot.car.zst' and output is '.', set the
                // destination to './snapshot.forest.car.zst'.
                let destination = match output_path.is_dir() {
                    true => {
                        let mut destination = output_path;
                        destination.push(source.clone());
                        while let Some(ext) = destination.extension() {
                            if !(ext == "zst" || ext == "car" || ext == "forest") {
                                break;
                            }
                            destination.set_extension("");
                        }
                        destination.with_extension("forest.car.zst")
                    }
                    false => output_path.clone(),
                };

                if !force && destination.exists() {
                    let have_permission = Confirm::with_theme(&ColorfulTheme::default())
                        .with_prompt(format!(
                            "{} will be overwritten. Continue?",
                            destination.to_string_lossy()
                        ))
                        .default(false)
                        .interact()
                        // e.g not a tty (or some other error), so haven't got permission.
                        .unwrap_or(false);
                    if !have_permission {
                        return Ok(());
                    }
                }

                println!("Generating ForestCAR.zst file: {:?}", &destination);

                let file = File::open(&source).await?;
                let pb = ProgressBar::new(file.metadata().await?.len()).with_style(
                    ProgressStyle::with_template("{bar} {percent}%, eta: {eta}")
                        .expect("infallible"),
                );
                let file = tokio::io::BufReader::new(pb.wrap_async_read(file));

                let mut block_stream = CarStream::new(file).await?;
                let roots = std::mem::take(&mut block_stream.header.roots);

                let mut dest = tokio::io::BufWriter::new(File::create(&destination).await?);

                let frames = crate::db::car::forest::Encoder::compress_stream(
                    frame_size,
                    compression_level,
                    block_stream.map_err(anyhow::Error::from),
                );
                crate::db::car::forest::Encoder::write(&mut dest, roots, frames).await?;
                dest.flush().await?;
                Ok(())
            }
        }
    }
}

/// Prints hex-encoded representation of SHA-256 checksum and saves it to a file
/// with the same name but with a `.sha256sum` extension.
async fn save_checksum(source: &Path, encoded_hash: String) -> Result<()> {
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
