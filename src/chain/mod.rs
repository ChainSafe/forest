// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod ec_finality;
mod snapshot_format;
pub mod store;
#[cfg(test)]
mod tests;
mod weight;

pub use self::{snapshot_format::*, store::*, weight::*};

use crate::blocks::{Tipset, TipsetKey};
use crate::chain::index::{ChainIndex, ResolveNullTipset};
use crate::cid_collections::{CidHashSet, CidHashSetLike, FileBackedCidHashSet};
use crate::db::IndexMapBlockstore;
use crate::db::car::forest::{self, ForestCarFrame, finalize_frame};
use crate::ipld::{IpldStream, should_save_block_to_snapshot, stream_chain};
use crate::prelude::*;
use crate::shim::executor::Receipt;
use crate::utils::db::car_stream::{CarBlock, CarBlockWrite};
use crate::utils::io::{AsyncWriterWithChecksum, Checksum};
use crate::utils::multihash::MultihashCode;
use crate::utils::stream::par_buffer;
use fil_actors_shared::fvm_ipld_hamt::Hamt;
use futures::{StreamExt as _, TryStreamExt as _};
use fvm_ipld_encoding::DAG_CBOR;
use multihash_derive::MultihashDigest as _;
use nunny::Vec as NonEmpty;
use sha2::digest::{self, Digest};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Instant;
use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter};
use tokio_util::task::AbortOnDropHandle;

pub const TIPSET_LOOKUP_HAMT_BIT_WIDTH: u32 = 5;

pub struct ExportOptions<S> {
    pub skip_checksum: bool,
    pub include_receipts: bool,
    pub include_events: bool,
    pub include_tipset_keys: bool,
    pub include_tipset_lookup: bool,
    pub seen: S,
    /// Ancestor of the exported tipset where the chain walk stops (exclusive)
    pub diff_base: Option<TipsetKey>,
}

impl<S: Default> Default for ExportOptions<S> {
    fn default() -> Self {
        Self {
            skip_checksum: Default::default(),
            include_receipts: Default::default(),
            include_events: Default::default(),
            include_tipset_keys: Default::default(),
            include_tipset_lookup: Default::default(),
            seen: Default::default(),
            diff_base: Default::default(),
        }
    }
}

pub struct ExportResult<D: Digest> {
    pub checksum: Option<digest::Output<D>>,
    #[allow(dead_code)]
    pub tipset_lookup: Option<anyhow::Result<Hamt<IndexMapBlockstore, TipsetKey, ChainEpoch>>>,
}

/// Oldest epoch whose state roots an export walks.
fn lookup_epoch_limit(
    tipset_epoch: ChainEpoch,
    lookup_depth: ChainEpoch,
) -> anyhow::Result<ChainEpoch> {
    tipset_epoch
        .checked_sub(lookup_depth)
        .with_context(|| format!("depth {lookup_depth} is out of range below epoch {tipset_epoch}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display)]
enum RootKind {
    #[strum(to_string = "messages")]
    Messages,
    #[strum(to_string = "state root")]
    StateRoot,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "{kind} {cid} at epoch {epoch} is missing, the database does not hold {depth} recent roots"
)]
pub(crate) struct MissingRecentRoot {
    kind: RootKind,
    cid: Cid,
    epoch: ChainEpoch,
    depth: ChainEpochDelta,
}

/// Fails fast when the oldest tipset an export walks lacks its roots, instead of hours into the export.
/// Only the roots are checked, as verifying the full trees would cost as much as the export.
async fn ensure_recent_roots_present(
    db: &(impl Blockstore + ShallowClone + Send + Sync + 'static),
    tipset: &Tipset,
    stateroot_lookup_limit: ChainEpoch,
) -> anyhow::Result<()> {
    let db = db.shallow_clone();
    let tipset = tipset.shallow_clone();
    let depth = tipset.epoch() - stateroot_lookup_limit;
    tokio::task::spawn_blocking(move || {
        let Some(oldest) = tipset
            .chain(&db)
            .take_while(|ts| ts.epoch() > stateroot_lookup_limit)
            .last()
        else {
            return Ok(());
        };
        let roots = std::iter::once((RootKind::StateRoot, *oldest.parent_state())).chain(
            oldest
                .block_headers()
                .iter()
                .map(|b| (RootKind::Messages, b.messages)),
        );
        for (kind, cid) in roots {
            if should_save_block_to_snapshot(cid) && !db.has(&cid)? {
                return Err(MissingRecentRoot {
                    kind,
                    cid,
                    epoch: oldest.epoch(),
                    depth,
                }
                .into());
            }
        }
        Ok(())
    })
    .await?
}

/// Builds `seen` from the tipset at `diff` down to `diff - diff_depth`, plus genesis.
pub async fn prepare_diff_export(
    db: &(impl Blockstore + ShallowClone + Unpin),
    index: &ChainIndex,
    tipset: &Tipset,
    diff: Option<ChainEpoch>,
    diff_depth: Option<ChainEpochDelta>,
    temp_dir: &Path,
) -> anyhow::Result<ExportOptions<FileBackedCidHashSet>> {
    let (seen, diff_base) = match diff {
        None => (FileBackedCidHashSet::new(temp_dir)?, None),
        Some(diff) => {
            anyhow::ensure!(
                diff < tipset.epoch(),
                "diff epoch {diff} must be smaller than target epoch {}",
                tipset.epoch()
            );
            let diff_ts = index
                .load_required_tipset_by_height(
                    diff,
                    tipset.shallow_clone(),
                    ResolveNullTipset::TakeOlder,
                )
                .await
                .context("diff epoch must be smaller than target epoch")?;
            let diff_base = diff_ts.key().clone();
            let diff_limit = diff_depth
                .map(|depth| lookup_epoch_limit(diff_ts.epoch(), depth))
                .transpose()
                .context("invalid diff depth")?
                .unwrap_or(0);
            tracing::info!(
                "building the diff base CID set, from epoch {} down to {diff_limit} and genesis...",
                diff_ts.epoch()
            );
            let start = Instant::now();
            // The export never reads headers below the base, but later state can share genesis state.
            let tipsets = diff_ts
                .chain_owned(db.shallow_clone())
                .take_while(move |ts| ts.epoch() > diff_limit.max(0))
                .chain(std::iter::once(index.genesis().shallow_clone()));
            let mut stream = stream_chain(
                db.shallow_clone(),
                tipsets,
                diff_limit,
                FileBackedCidHashSet::new(temp_dir)?,
            );
            while stream.try_next().await?.is_some() {}
            tracing::info!(
                "built the diff base CID set, took {}",
                humantime::format_duration(start.elapsed())
            );
            (stream.into_seen(), Some(diff_base))
        }
    };
    Ok(ExportOptions {
        skip_checksum: false,
        include_receipts: false,
        include_events: false,
        include_tipset_keys: false,
        include_tipset_lookup: false,
        seen,
        diff_base,
    })
}

/// Exports a Filecoin snapshot in v1 format
/// See <https://github.com/filecoin-project/FIPs/blob/98e33b9fa306959aa0131519eb4cc155522b2081/FRCs/frc-0108.md#v1-specification>
pub async fn export<D: Digest, S: CidHashSetLike + Send + Sync + 'static>(
    db: &(impl Blockstore + ShallowClone + Unpin + Send + Sync + 'static),
    tipset: &Tipset,
    lookup_depth: ChainEpochDelta,
    writer: impl AsyncWrite + Unpin,
    options: ExportOptions<S>,
) -> anyhow::Result<ExportResult<D>> {
    let roots = tipset.key().to_cids();
    export_to_forest_car::<D, S>(roots, None, db, tipset, lookup_depth, writer, options).await
}

/// Exports a Filecoin snapshot in v2 format
/// See <https://github.com/filecoin-project/FIPs/blob/98e33b9fa306959aa0131519eb4cc155522b2081/FRCs/frc-0108.md#v2-specification>
pub async fn export_v2<D: Digest, F: Seek + Read, S: CidHashSetLike + Send + Sync + 'static>(
    db: &(impl Blockstore + ShallowClone + Unpin + Send + Sync + 'static),
    mut f3: Option<(Cid, F)>,
    tipset: &Tipset,
    lookup_depth: ChainEpochDelta,
    writer: impl AsyncWrite + Unpin,
    options: ExportOptions<S>,
) -> anyhow::Result<ExportResult<D>> {
    // validate f3 data
    if let Some((f3_cid, f3_data)) = &mut f3 {
        f3_data.seek(SeekFrom::Start(0))?;
        let expected_cid = crate::f3::snapshot::get_f3_snapshot_cid(f3_data)?;
        anyhow::ensure!(
            f3_cid == &expected_cid,
            "f3 snapshot integrity check failed, actual cid: {f3_cid}, expected cid: {expected_cid}"
        );
    }

    let head = tipset.key().to_cids();
    let f3_cid = f3.as_ref().map(|(cid, _)| *cid);
    let snap_meta = FilecoinSnapshotMetadata::new_v2(head, f3_cid);
    let snap_meta_cbor_encoded = fvm_ipld_encoding::to_vec(&snap_meta)?;
    let snap_meta_block = CarBlock {
        cid: Cid::new_v1(
            DAG_CBOR,
            MultihashCode::Blake2b256.digest(&snap_meta_cbor_encoded),
        ),
        data: snap_meta_cbor_encoded.into(),
    };
    let roots = nunny::vec![snap_meta_block.cid];
    let mut prefix_data_frames = vec![{
        let mut encoder = forest::new_encoder(forest::DEFAULT_FOREST_CAR_COMPRESSION_LEVEL)?;
        snap_meta_block.write(&mut encoder)?;
        anyhow::Ok((
            vec![snap_meta_block.cid],
            finalize_frame(forest::DEFAULT_FOREST_CAR_COMPRESSION_LEVEL, &mut encoder)?,
        ))
    }];

    if let Some((f3_cid, mut f3_data)) = f3 {
        let f3_data_len = f3_data.seek(SeekFrom::End(0))?;
        f3_data.seek(SeekFrom::Start(0))?;
        prefix_data_frames.push({
            let mut encoder = forest::new_encoder(forest::DEFAULT_FOREST_CAR_COMPRESSION_LEVEL)?;
            encoder.write_car_block(f3_cid, f3_data_len, &mut f3_data)?;
            anyhow::Ok((
                vec![f3_cid],
                finalize_frame(forest::DEFAULT_FOREST_CAR_COMPRESSION_LEVEL, &mut encoder)?,
            ))
        });
    }

    export_to_forest_car::<D, S>(
        roots,
        Some(prefix_data_frames),
        db,
        tipset,
        lookup_depth,
        writer,
        options,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn export_to_forest_car<D: Digest, S: CidHashSetLike + Send + Sync + 'static>(
    roots: NonEmpty<Cid>,
    prefix_data_frames: Option<Vec<anyhow::Result<ForestCarFrame>>>,
    db: &(impl Blockstore + ShallowClone + Unpin + Send + Sync + 'static),
    tipset: &Tipset,
    lookup_depth: ChainEpochDelta,
    writer: impl AsyncWrite + Unpin,
    ExportOptions {
        skip_checksum,
        include_receipts,
        include_events,
        include_tipset_keys,
        include_tipset_lookup,
        seen,
        diff_base,
    }: ExportOptions<S>,
) -> anyhow::Result<ExportResult<D>> {
    if include_events && !include_receipts {
        anyhow::bail!("message receipts must be included when events are included");
    }
    anyhow::ensure!(
        diff_base.is_none() || !(include_tipset_keys || include_tipset_lookup),
        "tipset keys and tipset lookup are not supported in diff exports"
    );

    let start = Instant::now();
    tracing::info!(
        "Exporting snapshot, epoch={}, depth={lookup_depth}, prefix_frames={}",
        tipset.epoch(),
        prefix_data_frames.as_ref().map(|v| v.len()).unwrap_or(0)
    );

    let stateroot_lookup_limit =
        lookup_epoch_limit(tipset.epoch(), lookup_depth).context("invalid recent roots depth")?;
    ensure_recent_roots_present(db, tipset, stateroot_lookup_limit).await?;

    // Wrap writer in optional checksum calculator
    let mut writer = AsyncWriterWithChecksum::<D, _>::new(BufWriter::new(writer), !skip_checksum);

    let (ts_lookup_tx, ts_lookup_handle) = if include_tipset_lookup {
        let (ts_lookup_tx, ts_lookup_rx) = flume::bounded::<(ChainEpoch, TipsetKey)>(1024);
        let handle = AbortOnDropHandle::new(tokio::spawn(async move {
            let mut hamt = Hamt::new_with_bit_width(
                IndexMapBlockstore::default(),
                TIPSET_LOOKUP_HAMT_BIT_WIDTH,
            );
            while let Ok((epoch, tsk)) = ts_lookup_rx.recv_async().await {
                hamt.set(epoch, tsk)?;
            }
            hamt.flush()?;
            anyhow::Ok(hamt)
        }));
        (Some(ts_lookup_tx), Some(handle))
    } else {
        (None, None)
    };

    // Stream stateroots in range (stateroot_lookup_limit+1)..=tipset.epoch(). Also
    // stream all block headers until genesis, or until `diff_base` (exclusive) for diff exports.
    let (blocks, _drop_guard) = par_buffer(
        // Queue 1k blocks. This is enough to saturate the compressor and blocks
        // are small enough that keeping 1k in memory isn't a problem. Average
        // block size is between 1kb and 2kb.
        1024,
        stream_chain(
            db.shallow_clone(),
            tipset
                .shallow_clone()
                .chain_owned(db.shallow_clone())
                .take_while(move |ts| diff_base.as_ref() != Some(ts.key()))
                .inspect(move |ts| {
                    if let Some(ts_lookup_tx) = &ts_lookup_tx
                        && ChainIndex::is_tipset_lookup_checkpoint(ts.epoch())
                    {
                        _ = ts_lookup_tx.send((ts.epoch(), ts.key().clone()));
                    }
                }),
            stateroot_lookup_limit,
            seen,
        )
        .with_message_receipts(include_receipts)
        .with_events(include_events)
        .with_tipset_keys(include_tipset_keys)
        .track_progress(true),
    );

    // Encode Ipld key-value pairs in zstd frames
    let block_frames = forest::Encoder::compress_stream_default(blocks);
    let frames = futures::stream::iter(prefix_data_frames.unwrap_or_default()).chain(block_frames);

    // Write zstd frames and include a skippable index
    forest::Encoder::write(&mut writer, roots, frames).await?;

    // Flush to ensure everything has been successfully written
    tokio::time::timeout(forest::ASYNC_OPS_TIMEOUT, writer.flush())
        .await
        .context("`writer.flush` timed out")??;

    let digest = writer.finalize().map_err(|e| Error::Other(e.to_string()))?;

    let tipset_lookup = if let Some(ts_lookup_handle) = ts_lookup_handle {
        // This join is not I/O-bound: the task finishes once the `par_buffer` producer
        // exits and drops `ts_lookup_tx`, which `Encoder::write` guarantees by exhausting
        // the frame stream. The timeout guards against a pipeline lifecycle bug keeping a
        // sender alive, not against slowness.
        Some(
            tokio::time::timeout(forest::ASYNC_OPS_TIMEOUT, ts_lookup_handle)
                .await
                .context(
                    "tipset-lookup task did not finish; is a `ts_lookup_tx` sender still alive?",
                )??,
        )
    } else {
        None
    };

    tracing::info!(
        "Exported snapshot, took {}",
        humantime::format_duration(start.elapsed())
    );

    Ok(ExportResult {
        checksum: digest,
        tipset_lookup,
    })
}

pub async fn export_receipts_events_to_forest_car(
    db: &(impl Blockstore + ShallowClone + Unpin + Send + Sync + 'static),
    tipset: &Tipset,
    lookup_depth: ChainEpochDelta,
    writer: impl AsyncWrite + Unpin,
) -> anyhow::Result<()> {
    let start = Instant::now();
    tracing::info!(
        "Exporting message receipts and events snapshot, epoch={}, depth={lookup_depth}",
        tipset.epoch(),
    );

    let min_lookup_epoch_exclusive =
        lookup_epoch_limit(tipset.epoch(), lookup_depth).context("invalid recent roots depth")?;
    let ipld_roots = tokio::task::spawn_blocking({
        let tipset = tipset.shallow_clone();
        let db = db.shallow_clone();
        move || {
            // With 2k state trees on mainnet, it's `~2k receipt roots` + `~8k event roots` ~= `~10k ipld roots`.
            let mut ipld_roots = vec![];
            for ts in tipset
                .chain(&db)
                .take_while(|ts| ts.epoch() > min_lookup_epoch_exclusive)
            {
                let message_receipts_root = *ts.parent_message_receipts();
                ipld_roots.push(message_receipts_root);
                let receipts = Receipt::get_receipts(&db, message_receipts_root).with_context(|| {
                    format!(
                        "failed to get receipts, root: {message_receipts_root}, epoch: {}, tipset key: {}",
                        ts.epoch(),
                        ts.key(),
                    )
                })?;
                ipld_roots.extend(receipts.into_iter().filter_map(|r| r.events_root()));
            }
            anyhow::Ok(ipld_roots)
        }
    })
    .await??;

    let stream = IpldStream::new(db.shallow_clone(), ipld_roots, CidHashSet::default());
    let mut writer = BufWriter::new(writer);
    let (blocks, _drop_guard) = par_buffer(
        // Queue 1k blocks. This is enough to saturate the compressor and blocks
        // are small enough that keeping 1k in memory isn't a problem. Average
        // block size is between 1kb and 2kb.
        1024, stream,
    );
    // Encode Ipld key-value pairs in zstd frames
    let block_frames = forest::Encoder::compress_stream_default(blocks);

    // There's no data root for this snapshot, use a default CID as placeholder.
    // Note that the output CAR could only be validated with `forest-tool snapshot validate-extended`.
    // Another option could be including chain spine in the CAR and use head tipset key as root,
    // whose downside would be bloating the CAR.
    let roots = nunny::vec![Cid::default()];

    // Write zstd frames and include a skippable index
    forest::Encoder::write(&mut writer, roots, block_frames).await?;

    // Flush to ensure everything has been successfully written
    tokio::time::timeout(forest::ASYNC_OPS_TIMEOUT, writer.flush())
        .await
        .context("`writer.flush` timed out")??;

    tracing::info!(
        "Exported message receipts and events snapshot, took {}",
        humantime::format_duration(start.elapsed())
    );

    Ok(())
}
