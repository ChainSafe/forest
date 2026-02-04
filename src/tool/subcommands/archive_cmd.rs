// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Archives are key-value pairs encoded as
//! [CAR files](https://ipld.io/specs/transport/car/carv1/). The key-value pairs
//! represent a directed, acyclic graph (DAG). This graph is often a subset of a larger
//! graph and references to missing keys are common.
//!
//! Each graph contains blocks, messages, state trees, and miscellaneous data
//! such as compiled `WASM` code. The amount of data differs greatly in different
//! kinds of archives. While there are no fixed definitions, there are three
//! common kind of archives:
//! - A full archive contains a complete graph with no missing nodes. These
//!   archives are large (14 TiB for Filecoin's mainnet) and only used in special
//!   situations.
//! - A lite-archive typically has roughly 3 million blocks, 2000 complete sets of
//!   state-roots, and 2000 sets of messages. These archives usually take up
//!   roughly 100 GiB.
//! - A diff-archive contains the subset of nodes that are _not_ shared by two
//!   other archives. These archives are much smaller but can rarely be used on
//!   their own. They are typically merged with other archives before use.
//!
//! The sub-commands in this module manipulate archive files without needing a
//! running Forest-daemon or a separate database. Operations are carried out
//! directly on CAR files.
//!
//! Additional reading: [`crate::db::car::plain`]

use crate::blocks::Tipset;
use crate::chain::{
    ChainEpochDelta, ExportOptions, FilecoinSnapshotMetadata, FilecoinSnapshotVersion,
    index::{ChainIndex, ResolveNullTipset},
};
use crate::cid_collections::CidHashSet;
use crate::cli_shared::{snapshot, snapshot::TrustedVendor};
use crate::daemon::bundle::load_actor_bundles;
use crate::db::car::{AnyCar, ManyCar, forest::DEFAULT_FOREST_CAR_COMPRESSION_LEVEL};
use crate::f3::snapshot::F3SnapshotHeader;
use crate::interpreter::VMTrace;
use crate::ipld::{stream_chain, stream_graph};
use crate::networks::{ChainConfig, NetworkChain, butterflynet, calibnet, mainnet};
use crate::shim::address::CurrentNetwork;
use crate::shim::clock::{ChainEpoch, EPOCH_DURATION_SECONDS, EPOCHS_IN_DAY};
use crate::shim::fvm_shared_latest::address::Network;
use crate::shim::machine::GLOBAL_MULTI_ENGINE;
use crate::state_manager::{NO_CALLBACK, StateOutput, apply_block_messages};
use crate::utils::db::car_stream::{CarBlock, CarBlockWrite as _, CarStream};
use crate::utils::multihash::MultihashCode;
use anyhow::{Context as _, bail};
use chrono::DateTime;
use cid::Cid;
use clap::{Subcommand, ValueEnum};
use dialoguer::{Confirm, theme::ColorfulTheme};
use futures::{StreamExt as _, TryStreamExt as _};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::DAG_CBOR;
use indicatif::{ProgressBar, ProgressIterator, ProgressStyle};
use itertools::Itertools;
use multihash_derive::MultihashDigest as _;
use sha2::Sha256;
use std::fs::File;
use std::io::{BufReader, Seek as _, SeekFrom};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncWriteExt, BufWriter};
use tracing::info;

#[derive(Debug, Clone, ValueEnum)]
pub enum ExportMode {
    /// Export all types of snapshots.
    All,
    /// Export only lite snapshots.
    Lite,
    /// Export only diff snapshots.
    Diff,
}

impl ExportMode {
    pub fn lite(&self) -> bool {
        matches!(self, ExportMode::All | ExportMode::Lite)
    }

    pub fn diff(&self) -> bool {
        matches!(self, ExportMode::All | ExportMode::Diff)
    }
}

#[derive(Debug, Subcommand)]
pub enum ArchiveCommands {
    /// Show basic information about an archive.
    Info {
        /// Path to an archive (`.car` or `.car.zst`).
        snapshot: PathBuf,
    },
    /// Show FRC-0108 metadata of an Filecoin snapshot archive.
    Metadata {
        /// Path to an archive (`.car` or `.car.zst`).
        snapshot: PathBuf,
    },
    /// Show FRC-0108 header of a standalone F3 snapshot.
    F3Header {
        /// Path to a standalone F3 snapshot.
        snapshot: PathBuf,
    },
    /// Trim a snapshot of the chain and write it to `<output_path>`
    Export {
        /// Snapshot input path. Currently supports only `.car` file format.
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
        /// Snapshot output filename or directory. Defaults to
        /// `./forest_snapshot_{chain}_{year}-{month}-{day}_height_{epoch}.car.zst`.
        #[arg(short, long, default_value = ".", verbatim_doc_comment)]
        output_path: PathBuf,
        /// Latest epoch that has to be exported for this snapshot, the upper bound. This value
        /// cannot be greater than the latest epoch available in the input snapshot.
        #[arg(short, long)]
        epoch: Option<ChainEpoch>,
        /// How many state-roots to include. Lower limit is 900 for `calibnet` and `mainnet`.
        #[arg(short, long, default_value_t = 2000)]
        depth: ChainEpochDelta,
        /// Do not include any values reachable from this epoch.
        #[arg(long)]
        diff: Option<ChainEpoch>,
        /// How many state-roots to include when computing the diff set. All
        /// state-roots are included if this flag is not set.
        #[arg(long)]
        diff_depth: Option<ChainEpochDelta>,
        /// Overwrite output file without prompting.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// Print block headers at 30 day interval for a snapshot file
    Checkpoints {
        /// Path to snapshot file.
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
    },
    /// Merge snapshot archives into a single file. The output snapshot refers
    /// to the heaviest tipset in the input set.
    Merge {
        /// Snapshot input paths. Supports `.car`, `.car.zst`, and `.forest.car.zst`.
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
        /// Snapshot output filename or directory. Defaults to
        /// `./forest_snapshot_{chain}_{year}-{month}-{day}_height_{epoch}.car.zst`.
        #[arg(short, long, default_value = ".", verbatim_doc_comment)]
        output_path: PathBuf,
        /// Overwrite output file without prompting.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// Merge a v1 Filecoin snapshot with an F3 snapshot into a v2 Filecoin snapshot in `.forest.car.zst` format
    MergeF3 {
        /// Path to the v1 Filecoin snapshot
        #[arg(long = "v1")]
        filecoin_v1: PathBuf,
        /// Path to the F3 snapshot
        #[arg(long)]
        f3: PathBuf,
        /// Path to the snapshot output file in `.forest.car.zst` format
        #[arg(long)]
        output: PathBuf,
    },
    /// Show the difference between the canonical and computed state of a
    /// tipset.
    Diff {
        /// Snapshot input paths. Supports `.car`, `.car.zst`, and `.forest.car.zst`.
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
        /// Selected epoch to validate.
        #[arg(long)]
        epoch: ChainEpoch,
        // Depth of diffing. Differences in trees below this depth will just be
        // shown as different branch IDs.
        #[arg(long)]
        depth: Option<u64>,
    },
    /// Export lite and diff snapshots from one or more CAR files, and upload them
    /// to an `S3` bucket.
    SyncBucket {
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
        /// `S3` endpoint URL.
        #[arg(long, default_value = FOREST_ARCHIVE_S3_ENDPOINT)]
        endpoint: String,
        /// Don't generate or upload files, just show what would be done.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Export mode
        #[arg(long, value_enum, default_value_t = ExportMode::All)]
        export_mode: ExportMode,
    },
}

impl ArchiveCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Info { snapshot } => {
                let store = AnyCar::try_from(snapshot.as_path())?;
                let variant = store.variant().to_string();
                let heaviest = store.heaviest_tipset()?;
                let index_size_bytes = store.index_size_bytes();
                let snapshot_version = if let Some(metadata) = store.metadata() {
                    metadata.version
                } else {
                    FilecoinSnapshotVersion::V1
                };
                println!(
                    "{}",
                    ArchiveInfo::from_store(
                        &store,
                        variant,
                        heaviest,
                        snapshot_version,
                        index_size_bytes
                    )?
                );
                Ok(())
            }
            Self::Metadata { snapshot } => {
                let store = AnyCar::try_from(snapshot.as_path())?;
                if let Some(metadata) = store.metadata() {
                    println!("{metadata}");
                    if let Some(f3_cid) = metadata.f3_data {
                        let mut f3_data = store
                            .get_reader(f3_cid)?
                            .with_context(|| format!("f3 data not found, cid: {f3_cid}"))?;
                        let f3_snap_header = F3SnapshotHeader::decode_from_snapshot(&mut f3_data)?;
                        println!("{f3_snap_header}");
                    }
                } else {
                    println!(
                        "No metadata found (required by v2 snapshot) - this appears to be a v1 snapshot"
                    );
                }
                Ok(())
            }
            Self::F3Header { snapshot } => {
                let mut r = BufReader::new(File::open(&snapshot).with_context(|| {
                    format!("failed to open F3 snapshot '{}'", snapshot.display())
                })?);
                let f3_snap_header =
                    F3SnapshotHeader::decode_from_snapshot(&mut r).with_context(|| {
                        format!(
                            "failed to decode F3 snapshot header from '{}'",
                            snapshot.display()
                        )
                    })?;
                println!("{f3_snap_header}");
                Ok(())
            }
            Self::Export {
                snapshot_files,
                output_path,
                epoch,
                depth,
                diff,
                diff_depth,
                force,
            } => {
                let store = ManyCar::try_from(snapshot_files)?;
                let heaviest_tipset = store.heaviest_tipset()?;
                do_export(
                    &store.into(),
                    heaviest_tipset,
                    output_path,
                    epoch,
                    depth,
                    diff,
                    diff_depth,
                    force,
                )
                .await
            }
            Self::Checkpoints {
                snapshot_files: snapshot,
            } => print_checkpoints(snapshot),
            Self::Merge {
                snapshot_files,
                output_path,
                force,
            } => merge_snapshots(snapshot_files, output_path, force).await,
            Self::MergeF3 {
                filecoin_v1,
                f3,
                output,
            } => merge_f3_snapshot(filecoin_v1, f3, output).await,
            Self::Diff {
                snapshot_files,
                epoch,
                depth,
            } => show_tipset_diff(snapshot_files, epoch, depth).await,
            Self::SyncBucket {
                snapshot_files,
                endpoint,
                dry_run,
                export_mode,
            } => sync_bucket(snapshot_files, endpoint, dry_run, export_mode).await,
        }
    }
}

#[derive(Debug)]
pub struct ArchiveInfo {
    variant: String,
    network: String,
    epoch: ChainEpoch,
    tipsets: ChainEpoch,
    messages: ChainEpoch,
    head: Tipset,
    snapshot_version: FilecoinSnapshotVersion,
    index_size_bytes: Option<u32>,
}

impl std::fmt::Display for ArchiveInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "CAR format:       {}", self.variant)?;
        writeln!(f, "Snapshot version: {}", self.snapshot_version as u64)?;
        writeln!(f, "Network:          {}", self.network)?;
        writeln!(f, "Epoch:            {}", self.epoch)?;
        writeln!(f, "State-roots:      {}", self.epoch - self.tipsets + 1)?;
        writeln!(f, "Messages sets:    {}", self.epoch - self.messages + 1)?;
        let head_tipset_key_string = self
            .head
            .cids()
            .iter()
            .map(Cid::to_string)
            .join("\n                  ");
        write!(f, "Head Tipset:      {head_tipset_key_string}")?;
        if let Some(index_size_bytes) = self.index_size_bytes {
            writeln!(f)?;
            write!(
                f,
                "Index size:       {}",
                human_bytes::human_bytes(index_size_bytes)
            )?;
        }
        Ok(())
    }
}

impl ArchiveInfo {
    // Scan a CAR archive to identify which network it belongs to and how many
    // tipsets/messages are available. Progress is rendered to stdout.
    fn from_store(
        store: &impl Blockstore,
        variant: String,
        heaviest_tipset: Tipset,
        snapshot_version: FilecoinSnapshotVersion,
        index_size_bytes: Option<u32>,
    ) -> anyhow::Result<Self> {
        Self::from_store_with(
            store,
            variant,
            heaviest_tipset,
            snapshot_version,
            index_size_bytes,
            true,
        )
    }

    // Scan a CAR archive to identify which network it belongs to and how many
    // tipsets/messages are available. Progress is optionally rendered to
    // stdout.
    fn from_store_with(
        store: &impl Blockstore,
        variant: String,
        heaviest_tipset: Tipset,
        snapshot_version: FilecoinSnapshotVersion,
        index_size_bytes: Option<u32>,
        progress: bool,
    ) -> anyhow::Result<Self> {
        let head = heaviest_tipset;
        let root_epoch = head.epoch();

        let tipsets = head.clone().chain(store);

        let windowed = std::iter::once(head.clone()).chain(tipsets).tuple_windows();

        let mut network: String = "unknown".into();
        let mut lowest_stateroot_epoch = root_epoch;
        let mut lowest_message_epoch = root_epoch;

        let iter = if progress {
            itertools::Either::Left(windowed.progress_count(root_epoch as u64))
        } else {
            itertools::Either::Right(windowed)
        };

        for (parent, tipset) in iter {
            if tipset.epoch() >= parent.epoch() && parent.epoch() != root_epoch {
                bail!("Broken invariant: non-sequential epochs");
            }

            if tipset.epoch() < 0 {
                bail!("Broken invariant: tipset with negative epoch");
            }

            // Update the lowest-stateroot-epoch only if our parent also has a
            // state-root. The genesis state-root is usually available but we're
            // not interested in that.
            if lowest_stateroot_epoch == parent.epoch() && store.has(tipset.parent_state())? {
                lowest_stateroot_epoch = tipset.epoch();
            }
            if lowest_message_epoch == parent.epoch()
                && store.has(&tipset.min_ticket_block().messages)?
            {
                lowest_message_epoch = tipset.epoch();
            }

            let mut update_network_name = |block_cid: &Cid| {
                if block_cid == &*calibnet::GENESIS_CID {
                    network = calibnet::NETWORK_COMMON_NAME.into();
                } else if block_cid == &*mainnet::GENESIS_CID {
                    network = mainnet::NETWORK_COMMON_NAME.into();
                } else if block_cid == &*butterflynet::GENESIS_CID {
                    network = butterflynet::NETWORK_COMMON_NAME.into();
                }
            };

            if tipset.epoch() == 0 {
                let block_cid = tipset.min_ticket_block().cid();
                update_network_name(block_cid);
            }

            // If we've already found the lowest-stateroot-epoch and
            // lowest-message-epoch then we can skip scanning the rest of the
            // archive when we find a checkpoint.
            let may_skip =
                lowest_stateroot_epoch != tipset.epoch() && lowest_message_epoch != tipset.epoch();
            if may_skip {
                let genesis_block = tipset.genesis(&store)?;
                update_network_name(genesis_block.cid());
                break;
            }
        }

        Ok(ArchiveInfo {
            variant,
            network,
            epoch: root_epoch,
            tipsets: lowest_stateroot_epoch,
            messages: lowest_message_epoch,
            head,
            snapshot_version,
            index_size_bytes,
        })
    }

    fn epoch_range(&self) -> Range<ChainEpoch> {
        self.tipsets..self.epoch
    }
}

// Print a mapping of epochs to block headers in yaml format. This mapping can
// be used by Forest to quickly identify tipsets.
fn print_checkpoints(snapshot_files: Vec<PathBuf>) -> anyhow::Result<()> {
    let store = ManyCar::try_from(snapshot_files).context("couldn't read input CAR file")?;
    let root = store.heaviest_tipset()?;

    let genesis = root.genesis(&store)?;
    let chain_name =
        NetworkChain::from_genesis(genesis.cid()).context("Unrecognizable genesis block")?;

    println!("{chain_name}:");
    for (epoch, cid) in list_checkpoints(&store, root) {
        println!("  {epoch}: {cid}");
    }
    Ok(())
}

fn list_checkpoints(
    db: &impl Blockstore,
    root: Tipset,
) -> impl Iterator<Item = (ChainEpoch, cid::Cid)> + '_ {
    let interval = EPOCHS_IN_DAY * 30;
    let mut target_epoch = root.epoch() - root.epoch() % interval;
    root.chain(db).filter_map(move |tipset| {
        if tipset.epoch() <= target_epoch && tipset.epoch() != 0 {
            target_epoch -= interval;
            Some((tipset.epoch(), *tipset.min_ticket_block().cid()))
        } else {
            None
        }
    })
}

// This does nothing if the output path is a file. If it is a directory - it produces the following:
// `./forest_snapshot_{chain}_{year}-{month}-{day}_height_{epoch}.car.zst`.
fn build_output_path(
    chain: String,
    genesis_timestamp: u64,
    epoch: ChainEpoch,
    output_path: PathBuf,
) -> PathBuf {
    match output_path.is_dir() {
        true => output_path.join(snapshot::filename(
            TrustedVendor::Forest,
            chain,
            DateTime::from_timestamp(genesis_timestamp as i64 + epoch * EPOCH_DURATION_SECONDS, 0)
                .unwrap_or_default()
                .naive_utc()
                .date(),
            epoch,
            true,
        )),
        false => output_path.clone(),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn do_export(
    store: &Arc<impl Blockstore + Send + Sync + 'static>,
    root: Tipset,
    output_path: PathBuf,
    epoch_option: Option<ChainEpoch>,
    depth: ChainEpochDelta,
    diff: Option<ChainEpoch>,
    diff_depth: Option<ChainEpochDelta>,
    force: bool,
) -> anyhow::Result<()> {
    let ts = root;

    let genesis = ts.genesis(store)?;
    let network = NetworkChain::from_genesis_or_devnet_placeholder(genesis.cid());

    let epoch = epoch_option.unwrap_or(ts.epoch());

    let finality = ChainConfig::from_chain(&network)
        .policy
        .chain_finality
        .min(epoch);
    if depth < finality {
        bail!("For {}, depth has to be at least {}.", network, finality);
    }

    info!("looking up a tipset by epoch: {}", epoch);

    let index = ChainIndex::new(store.clone());

    let ts = index
        .tipset_by_height(epoch, ts, ResolveNullTipset::TakeOlder)
        .context("unable to get a tipset at given height")?;

    let seen = if let Some(diff) = diff {
        let diff_ts: Tipset = index
            .tipset_by_height(diff, ts.clone(), ResolveNullTipset::TakeOlder)
            .context("diff epoch must be smaller than target epoch")?;
        let diff_ts: &Tipset = &diff_ts;
        let diff_limit = diff_depth.map(|depth| diff_ts.epoch() - depth).unwrap_or(0);
        let mut stream = stream_chain(
            store.clone(),
            diff_ts.clone().chain_owned(store.clone()),
            diff_limit,
        );
        while stream.try_next().await?.is_some() {}
        stream.into_seen()
    } else {
        CidHashSet::default()
    };

    let output_path = build_output_path(network.to_string(), genesis.timestamp, epoch, output_path);

    if !force && output_path.exists() {
        let have_permission = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "{} will be overwritten. Continue?",
                output_path.to_string_lossy()
            ))
            .default(false)
            .interact()
            // e.g not a tty (or some other error), so haven't got permission.
            .unwrap_or(false);
        if !have_permission {
            return Ok(());
        }
    }

    let writer = tokio::fs::File::create(&output_path)
        .await
        .with_context(|| {
            format!(
                "unable to create a snapshot - is the output path '{}' correct?",
                output_path.to_str().unwrap_or_default()
            )
        })?;

    info!(
        "exporting snapshot at location: {}",
        output_path.to_str().unwrap_or_default()
    );

    let pb = ProgressBar::new_spinner().with_style(
        ProgressStyle::with_template(
            "{spinner} exported {total_bytes} with {binary_bytes_per_sec} in {elapsed}",
        )
        .expect("indicatif template must be valid"),
    );
    pb.enable_steady_tick(std::time::Duration::from_secs_f32(0.1));
    let writer = pb.wrap_async_write(writer);

    crate::chain::export::<Sha256>(
        store,
        &ts,
        depth,
        writer,
        Some(ExportOptions {
            skip_checksum: true,
            seen,
        }),
    )
    .await?;

    Ok(())
}

// TODO(lemmih): https://github.com/ChainSafe/forest/issues/3347
//               Testing with diff snapshots can be significantly improved
/// Merge a set of snapshots (diff snapshots or lite snapshots). The output
/// snapshot links to the heaviest tipset in the input set.
async fn merge_snapshots(
    snapshot_files: Vec<PathBuf>,
    output_path: PathBuf,
    force: bool,
) -> anyhow::Result<()> {
    use crate::db::car::forest;

    let store = ManyCar::try_from(snapshot_files)?;
    let heaviest_tipset = store.heaviest_tipset()?;
    let roots = heaviest_tipset.key().to_cids();

    if !force && output_path.exists() {
        let have_permission = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "{} will be overwritten. Continue?",
                output_path.to_string_lossy()
            ))
            .default(false)
            .interact()
            // e.g not a tty (or some other error), so haven't got permission.
            .unwrap_or(false);
        if !have_permission {
            return Ok(());
        }
    }

    let mut writer = BufWriter::new(tokio::fs::File::create(&output_path).await.context(
        format!(
            "unable to create a snapshot - is the output path '{}' correct?",
            output_path.to_str().unwrap_or_default()
        ),
    )?);

    // Stream all available blocks from heaviest_tipset to genesis.
    let blocks = stream_graph(&store, heaviest_tipset.chain(&store), 0);

    // Encode Ipld key-value pairs in zstd frames
    let frames = forest::Encoder::compress_stream_default(blocks);

    // Write zstd frames and include a skippable index
    forest::Encoder::write(&mut writer, roots, frames).await?;

    // Flush to ensure everything has been successfully written
    writer.flush().await.context("failed to flush")?;

    Ok(())
}

async fn merge_f3_snapshot(filecoin: PathBuf, f3: PathBuf, output: PathBuf) -> anyhow::Result<()> {
    let store = AnyCar::try_from(filecoin.as_path())?;
    anyhow::ensure!(
        store.metadata().is_none(),
        "The filecoin snapshot is not in v1 format"
    );
    drop(store);

    let mut f3_data = File::open(f3)?;
    let f3_cid = crate::f3::snapshot::get_f3_snapshot_cid(&mut f3_data)?;

    let car_stream = CarStream::new_from_path(&filecoin).await?;
    let chain_head = car_stream.head_tipset_key().to_cids();

    println!("f3 snapshot cid: {f3_cid}");
    println!(
        "chain head:      [{}]",
        chain_head.iter().map(|c| c.to_string()).join(", ")
    );

    let snap_meta = FilecoinSnapshotMetadata::new_v2(chain_head, Some(f3_cid));
    let snap_meta_cbor_encoded = fvm_ipld_encoding::to_vec(&snap_meta)?;
    let snap_meta_block = CarBlock {
        cid: Cid::new_v1(
            DAG_CBOR,
            MultihashCode::Blake2b256.digest(&snap_meta_cbor_encoded),
        ),
        data: snap_meta_cbor_encoded,
    };

    let roots = nunny::vec![snap_meta_block.cid];
    let snap_meta_frame = {
        let mut encoder =
            crate::db::car::forest::new_encoder(DEFAULT_FOREST_CAR_COMPRESSION_LEVEL)?;
        snap_meta_block.write(&mut encoder)?;
        anyhow::Ok((
            vec![snap_meta_block.cid],
            crate::db::car::forest::finalize_frame(
                DEFAULT_FOREST_CAR_COMPRESSION_LEVEL,
                &mut encoder,
            )?,
        ))
    };
    let f3_frame = {
        let mut encoder =
            crate::db::car::forest::new_encoder(DEFAULT_FOREST_CAR_COMPRESSION_LEVEL)?;
        let f3_data_len = f3_data.seek(SeekFrom::End(0))?;
        f3_data.seek(SeekFrom::Start(0))?;
        encoder.write_car_block(f3_cid, f3_data_len, &mut f3_data)?;
        anyhow::Ok((
            vec![f3_cid],
            crate::db::car::forest::finalize_frame(
                DEFAULT_FOREST_CAR_COMPRESSION_LEVEL,
                &mut encoder,
            )?,
        ))
    };

    let block_frames = crate::db::car::forest::Encoder::compress_stream_default(
        car_stream.map_err(anyhow::Error::from),
    );
    let frames = futures::stream::iter([snap_meta_frame, f3_frame]).chain(block_frames);

    let temp_output = {
        let mut dir = output.clone();
        if dir.pop() {
            tempfile::NamedTempFile::new_in(dir)?
        } else {
            tempfile::NamedTempFile::new_in(".")?
        }
    };
    let writer = tokio::io::BufWriter::new(tokio::fs::File::create(&temp_output).await?);
    let pb = ProgressBar::new_spinner().with_style(
        ProgressStyle::with_template(
            "{spinner} {msg} {binary_total_bytes} written in {elapsed} ({binary_bytes_per_sec})",
        )
        .expect("indicatif template must be valid"),
    ).with_message(format!("Merging into {} ...", output.display()));
    pb.enable_steady_tick(std::time::Duration::from_secs(1));
    let mut writer = pb.wrap_async_write(writer);
    crate::db::car::forest::Encoder::write(&mut writer, roots, frames).await?;
    writer.shutdown().await?;
    temp_output.persist(&output)?;
    pb.finish();

    Ok(())
}

/// Compute the tree of actor states for a given epoch and compare it to the
/// expected result (as encoded in the blockchain). Differences are printed
/// using the diff format (red for the blockchain state, green for the computed
/// state).
async fn show_tipset_diff(
    snapshot_files: Vec<PathBuf>,
    epoch: ChainEpoch,
    depth: Option<u64>,
) -> anyhow::Result<()> {
    use colored::*;

    let store = Arc::new(ManyCar::try_from(snapshot_files)?);

    let heaviest_tipset = store.heaviest_tipset()?;
    if heaviest_tipset.epoch() <= epoch {
        anyhow::bail!(
            "Highest epoch must be at least 1 greater than the target epoch. \
             Highest epoch = {}, target epoch = {}.",
            heaviest_tipset.epoch(),
            epoch
        )
    }

    let genesis = heaviest_tipset.genesis(&store)?;
    let network = NetworkChain::from_genesis_or_devnet_placeholder(genesis.cid());
    load_actor_bundles(&store, &network).await?;

    let timestamp = genesis.timestamp;
    let chain_index = ChainIndex::new(Arc::clone(&store));
    let chain_config = ChainConfig::from_chain(&network);
    if chain_config.is_testnet() {
        CurrentNetwork::set_global(Network::Testnet);
    }
    let beacon = Arc::new(chain_config.get_beacon_schedule(timestamp));
    let tipset = chain_index.tipset_by_height(
        epoch,
        heaviest_tipset.clone(),
        ResolveNullTipset::TakeOlder,
    )?;

    let child_tipset = chain_index.tipset_by_height(
        epoch + 1,
        heaviest_tipset.clone(),
        ResolveNullTipset::TakeNewer,
    )?;

    let StateOutput { state_root, .. } = apply_block_messages(
        timestamp,
        Arc::new(chain_index),
        Arc::new(chain_config),
        beacon,
        &GLOBAL_MULTI_ENGINE,
        tipset,
        NO_CALLBACK,
        VMTrace::NotTraced,
    )?;

    if child_tipset.parent_state() != &state_root {
        println!(
            "{}",
            format!("- Expected state hash: {}", child_tipset.parent_state()).red()
        );
        println!("{}", format!("+ Computed state hash: {state_root}").green());

        crate::statediff::print_state_diff(
            &store,
            &state_root,
            child_tipset.parent_state(),
            depth,
        )?;
    } else {
        println!("Computed state matches expected state.");
    }

    Ok(())
}

fn steps_in_range(
    range: &Range<ChainEpoch>,
    step_size: ChainEpochDelta,
    offset: ChainEpochDelta,
) -> impl Iterator<Item = ChainEpoch> {
    let start = range.start / step_size;
    (start..)
        .map(move |x| x * step_size)
        .skip_while(move |&x| x - offset < range.start)
        .take_while(move |&x| x <= range.end)
}

fn epoch_to_date(genesis_timestamp: u64, epoch: ChainEpoch) -> anyhow::Result<String> {
    Ok(
        DateTime::from_timestamp(genesis_timestamp as i64 + epoch * EPOCH_DURATION_SECONDS, 0)
            .unwrap_or_default()
            .format("%Y-%m-%d")
            .to_string(),
    )
}

fn format_lite_snapshot(
    network: &str,
    genesis_timestamp: u64,
    epoch: ChainEpoch,
) -> anyhow::Result<String> {
    Ok(format!(
        "forest_snapshot_{network}_{date}_height_{epoch}.forest.car.zst",
        date = epoch_to_date(genesis_timestamp, epoch)?,
        epoch = epoch
    ))
}

fn format_diff_snapshot(
    network: &str,
    genesis_timestamp: u64,
    epoch: ChainEpoch,
) -> anyhow::Result<String> {
    Ok(format!(
        "forest_diff_{network}_{date}_height_{epoch}+3000.forest.car.zst",
        date = epoch_to_date(genesis_timestamp, epoch)?,
        epoch = epoch - 3000
    ))
}

// Check if
// forest-internal.chainsafe.dev/{network}/lite/forest_snapshot_{network}_{date}_height_{epoch}.forest.car.zst
// exists.
async fn bucket_has_lite_snapshot(
    network: &str,
    genesis_timestamp: u64,
    epoch: ChainEpoch,
) -> anyhow::Result<bool> {
    let url = format!(
        "https://forest-internal.chainsafe.dev/{}/lite/{}",
        network,
        format_lite_snapshot(network, genesis_timestamp, epoch)?
    );
    let response = reqwest::Client::new().get(url).send().await?;
    Ok(response.status().is_success())
}

// Check if
// forest-internal.chainsafe.dev/{network}/diff/forest_diff_{network}_{date}_height_{epoch}+3000.forest.car.zst
// exists.
async fn bucket_has_diff_snapshot(
    network: &str,
    genesis_timestamp: u64,
    epoch: ChainEpoch,
) -> anyhow::Result<bool> {
    let url = format!(
        "https://forest-internal.chainsafe.dev/{}/diff/{}",
        network,
        format_diff_snapshot(network, genesis_timestamp, epoch)?
    );
    let response = reqwest::Client::new().head(url).send().await?;
    Ok(response.status().is_success())
}

const FOREST_ARCHIVE_S3_ENDPOINT: &str =
    "https://2238a825c5aca59233eab1f221f7aefb.r2.cloudflarestorage.com";

/// Check if the AWS CLI is installed and correctly configured.
fn check_aws_config(endpoint: &str) -> anyhow::Result<()> {
    let status = std::process::Command::new("aws")
        .arg("help")
        .stdout(std::process::Stdio::null())
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to execute 'aws help': {}", e))?;

    if !status.success() {
        bail!(
            "'aws help' failed with status code: {}. Please ensure that the AWS CLI is installed and configured.",
            status
        );
    }

    let status = std::process::Command::new("aws")
        .args(["s3", "ls", "s3://forest-archive/", "--endpoint", endpoint])
        .stdout(std::process::Stdio::null())
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to execute 'aws s3 ls': {}", e))?;

    if !status.success() {
        bail!(
            "'aws s3 ls' failed with status code: {}. Please check your AWS credentials.",
            status
        );
    }
    Ok(())
}

/// Use the AWS CLI to upload a snapshot file to the `S3` bucket.
fn upload_to_forest_bucket(path: PathBuf, network: &str, tag: &str) -> anyhow::Result<()> {
    let status = std::process::Command::new("aws")
        .args([
            "s3",
            "cp",
            "--acl",
            "public-read",
            path.to_str().unwrap(),
            &format!("s3://forest-archive/{network}/{tag}/"),
            "--endpoint",
            FOREST_ARCHIVE_S3_ENDPOINT,
        ])
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to execute 'aws s3 cp': {}", e))?;

    if !status.success() {
        bail!(
            "'aws s3 cp' failed with status code: {}. Upload failed.",
            status
        );
    }
    Ok(())
}

/// Given a block store, export a lite snapshot for a given epoch.
async fn export_lite_snapshot(
    store: Arc<impl Blockstore + Send + Sync + 'static>,
    root: Tipset,
    network: &str,
    genesis_timestamp: u64,
    epoch: ChainEpoch,
) -> anyhow::Result<PathBuf> {
    let output_path: PathBuf = format_lite_snapshot(network, genesis_timestamp, epoch)?.into();

    // Skip if file already exists
    if output_path.exists() {
        return Ok(output_path);
    }

    let depth = 900;
    let diff = None;
    let diff_depth = None;
    let force = false;
    do_export(
        &store,
        root,
        output_path.clone(),
        Some(epoch),
        depth,
        diff,
        diff_depth,
        force,
    )
    .await?;
    Ok(output_path)
}

/// Given a block store, export a diff snapshot for a given epoch.
async fn export_diff_snapshot(
    store: Arc<impl Blockstore + Send + Sync + 'static>,
    root: Tipset,
    network: &str,
    genesis_timestamp: u64,
    epoch: ChainEpoch,
) -> anyhow::Result<PathBuf> {
    let output_path: PathBuf = format_diff_snapshot(network, genesis_timestamp, epoch)?.into();

    // Skip if file already exists
    if output_path.exists() {
        return Ok(output_path);
    }

    let depth = 3_000;
    let diff = Some(epoch - depth);
    let diff_depth = Some(900);
    let force = false;
    do_export(
        &store,
        root,
        output_path.clone(),
        Some(epoch),
        depth,
        diff,
        diff_depth,
        force,
    )
    .await?;
    Ok(output_path)
}

// This command is used for keeping the S3 bucket of archival snapshots
// up-to-date. It takes a set of snapshot files and queries the S3 bucket to see
// what is missing. If the input set of snapshot files can be used to generate
// missing lite or diff snapshots, they'll be generated and uploaded to the S3
// bucket.
async fn sync_bucket(
    snapshot_files: Vec<PathBuf>,
    endpoint: String,
    dry_run: bool,
    export_mode: ExportMode,
) -> anyhow::Result<()> {
    check_aws_config(&endpoint)?;

    let store = Arc::new(ManyCar::try_from(snapshot_files)?);
    let heaviest_tipset = store.heaviest_tipset()?;

    let info = ArchiveInfo::from_store(
        &store,
        "ManyCAR".to_string(),
        heaviest_tipset.clone(),
        FilecoinSnapshotVersion::V1,
        None,
    )?;

    let genesis_timestamp = heaviest_tipset.genesis(&store)?.timestamp;

    let range = info.epoch_range();

    println!("Network: {}", info.network);
    println!("Range:   {} to {}", range.start, range.end);
    if export_mode.lite() {
        println!("Lites:",);
        for epoch in steps_in_range(&range, 30_000, 800) {
            println!(
                "  {}: {}",
                epoch,
                bucket_has_lite_snapshot(&info.network, genesis_timestamp, epoch).await?
            );
        }
    }
    if export_mode.diff() {
        println!("Diffs:");
        for epoch in steps_in_range(&range, 3_000, 3_800) {
            println!(
                "  {}: {}",
                epoch,
                bucket_has_diff_snapshot(&info.network, genesis_timestamp, epoch).await?
            );
        }
    }

    if export_mode.lite() {
        for epoch in steps_in_range(&range, 30_000, 800) {
            if !bucket_has_lite_snapshot(&info.network, genesis_timestamp, epoch).await? {
                println!("  {epoch}: Exporting lite snapshot",);
                if !dry_run {
                    let output_path = export_lite_snapshot(
                        store.clone(),
                        heaviest_tipset.clone(),
                        &info.network,
                        genesis_timestamp,
                        epoch,
                    )
                    .await?;
                    upload_to_forest_bucket(output_path, &info.network, "lite")?;
                } else {
                    println!("  {epoch}: Would upload lite snapshot to S3");
                }
            }
        }
    }

    if export_mode.diff() {
        for epoch in steps_in_range(&range, 3_000, 3_800) {
            if !bucket_has_diff_snapshot(&info.network, genesis_timestamp, epoch).await? {
                println!("  {epoch}: Exporting diff snapshot",);
                if !dry_run {
                    let output_path = export_diff_snapshot(
                        store.clone(),
                        heaviest_tipset.clone(),
                        &info.network,
                        genesis_timestamp,
                        epoch,
                    )
                    .await?;
                    upload_to_forest_bucket(output_path, &info.network, "diff")?;
                } else {
                    println!("  {epoch}: Would upload diff snapshot to S3");
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::car::AnyCar;
    use crate::utils::db::car_stream::CarStream;
    use tempfile::TempDir;
    use tokio::io::BufReader;

    fn genesis_timestamp(genesis_car: &'static [u8]) -> u64 {
        let db = crate::db::car::PlainCar::try_from(genesis_car).unwrap();
        let ts = db.heaviest_tipset().unwrap();
        ts.genesis(&db).unwrap().timestamp
    }

    #[test]
    fn steps_in_range_1() {
        let range = 30_000..60_001;
        let lite = steps_in_range(&range, 30_000, 800);
        assert_eq!(lite.collect_vec(), vec![60_000]);
    }

    #[test]
    fn steps_in_range_2() {
        let range = (30_000 - 800)..60_001;
        let lite = steps_in_range(&range, 30_000, 800);
        assert_eq!(lite.collect_vec(), vec![30_000, 60_000]);
    }

    #[tokio::test]
    async fn export() {
        let output_path = TempDir::new().unwrap();
        let store = AnyCar::try_from(calibnet::DEFAULT_GENESIS).unwrap();
        let heaviest_tipset = store.heaviest_tipset().unwrap();
        do_export(
            &store.into(),
            heaviest_tipset,
            output_path.path().into(),
            Some(0),
            1,
            None,
            None,
            false,
        )
        .await
        .unwrap();
        let file = tokio::fs::File::open(build_output_path(
            NetworkChain::Calibnet.to_string(),
            genesis_timestamp(calibnet::DEFAULT_GENESIS),
            0,
            output_path.path().into(),
        ))
        .await
        .unwrap();
        CarStream::new(BufReader::new(file)).await.unwrap();
    }

    #[test]
    fn archive_info_calibnet() {
        let store = AnyCar::try_from(calibnet::DEFAULT_GENESIS).unwrap();
        let variant = store.variant().to_string();
        let ts = store.heaviest_tipset().unwrap();
        let index_size_bytes = store.index_size_bytes();
        let info = ArchiveInfo::from_store(
            &store,
            variant,
            ts,
            FilecoinSnapshotVersion::V1,
            index_size_bytes,
        )
        .unwrap();
        assert_eq!(info.network, "calibnet");
        assert_eq!(info.epoch, 0);
    }

    #[test]
    fn archive_info_mainnet() {
        let store = AnyCar::try_from(mainnet::DEFAULT_GENESIS).unwrap();
        let variant = store.variant().to_string();
        let ts = store.heaviest_tipset().unwrap();
        let index_size_bytes = store.index_size_bytes();
        let info = ArchiveInfo::from_store(
            &store,
            variant,
            ts,
            FilecoinSnapshotVersion::V1,
            index_size_bytes,
        )
        .unwrap();
        assert_eq!(info.network, "mainnet");
        assert_eq!(info.epoch, 0);
    }
}
