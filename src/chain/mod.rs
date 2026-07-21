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
use crate::chain::index::ChainIndex;
use crate::cid_collections::{CidHashSet, CidHashSetLike};
use crate::db::IndexMapBlockstore;
use crate::db::car::forest::{self, ForestCarFrame, finalize_frame};
use crate::ipld::{IpldStream, stream_chain};
use crate::prelude::*;
use crate::shim::executor::Receipt;
use crate::utils::db::car_stream::{CarBlock, CarBlockWrite};
use crate::utils::io::{AsyncWriterWithChecksum, Checksum};
use crate::utils::multihash::MultihashCode;
use crate::utils::stream::par_buffer;
use fil_actors_shared::fvm_ipld_hamt::Hamt;
use futures::StreamExt as _;
use fvm_ipld_encoding::DAG_CBOR;
use multihash_derive::MultihashDigest as _;
use nunny::Vec as NonEmpty;
use sha2::digest::{self, Digest};
use std::io::{Read, Seek, SeekFrom};
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
        }
    }
}

pub struct ExportResult<D: Digest> {
    pub checksum: Option<digest::Output<D>>,
    #[allow(dead_code)]
    pub tipset_lookup: Option<anyhow::Result<Hamt<IndexMapBlockstore, TipsetKey, ChainEpoch>>>,
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
    }: ExportOptions<S>,
) -> anyhow::Result<ExportResult<D>> {
    if include_events && !include_receipts {
        anyhow::bail!("message receipts must be included when events are included");
    }

    let start = Instant::now();
    tracing::info!(
        "Exporting snapshot, epoch={}, depth={lookup_depth}, prefix_frames={}",
        tipset.epoch(),
        prefix_data_frames.as_ref().map(|v| v.len()).unwrap_or(0)
    );

    let stateroot_lookup_limit = tipset.epoch() - lookup_depth;

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
    // stream all block headers until genesis.
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

    let min_lookup_epoch_exclusive = tipset.epoch() - lookup_depth;
    let ipld_roots = tokio::task::spawn_blocking({
        let tipset = tipset.shallow_clone();
        let db = db.shallow_clone();
        move || {
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

    // Stream message receipts and events in range (stateroot_lookup_limit+1)..=tipset.epoch().
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
