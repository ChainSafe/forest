// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod snapshot_format;
pub mod store;
#[cfg(test)]
mod tests;
mod weight;

pub use self::{snapshot_format::*, store::*, weight::*};

use crate::blocks::{Tipset, TipsetKey};
use crate::cid_collections::CidHashSet;
use crate::db::car::forest::{self, ForestCarFrame, finalize_frame};
use crate::db::{SettingsStore, SettingsStoreExt};
use crate::ipld::stream_chain;
use crate::utils::db::car_stream::{CarBlock, CarBlockWrite};
use crate::utils::io::{AsyncWriterWithChecksum, Checksum};
use crate::utils::multihash::MultihashCode;
use crate::utils::stream::par_buffer;
use anyhow::Context as _;
use cid::Cid;
use futures::StreamExt as _;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::DAG_CBOR;
use multihash_derive::MultihashDigest as _;
use nunny::Vec as NonEmpty;
use sha2::digest::{self, Digest};
use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;
use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter};

#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    pub skip_checksum: bool,
    pub seen: CidHashSet,
}

pub async fn export_from_head<D: Digest>(
    db: &Arc<impl Blockstore + SettingsStore + Send + Sync + 'static>,
    lookup_depth: ChainEpochDelta,
    writer: impl AsyncWrite + Unpin,
    options: Option<ExportOptions>,
) -> anyhow::Result<(Tipset, Option<digest::Output<D>>)> {
    let head_key = SettingsStoreExt::read_obj::<TipsetKey>(db, crate::db::setting_keys::HEAD_KEY)?
        .context("chain head key not found")?;
    let head_ts = Tipset::load_required(&db, &head_key)?;
    let digest = export::<D>(db, &head_ts, lookup_depth, writer, options).await?;
    Ok((head_ts, digest))
}

/// Exports a Filecoin snapshot in v1 format
/// See <https://github.com/filecoin-project/FIPs/blob/98e33b9fa306959aa0131519eb4cc155522b2081/FRCs/frc-0108.md#v1-specification>
pub async fn export<D: Digest>(
    db: &Arc<impl Blockstore + Send + Sync + 'static>,
    tipset: &Tipset,
    lookup_depth: ChainEpochDelta,
    writer: impl AsyncWrite + Unpin,
    options: Option<ExportOptions>,
) -> anyhow::Result<Option<digest::Output<D>>> {
    let roots = tipset.key().to_cids();
    export_to_forest_car::<D>(roots, None, db, tipset, lookup_depth, writer, options).await
}

/// Exports a Filecoin snapshot in v2 format
/// See <https://github.com/filecoin-project/FIPs/blob/98e33b9fa306959aa0131519eb4cc155522b2081/FRCs/frc-0108.md#v2-specification>
pub async fn export_v2<D: Digest, F: Seek + Read>(
    db: &Arc<impl Blockstore + Send + Sync + 'static>,
    mut f3: Option<(Cid, F)>,
    tipset: &Tipset,
    lookup_depth: ChainEpochDelta,
    writer: impl AsyncWrite + Unpin,
    options: Option<ExportOptions>,
) -> anyhow::Result<Option<digest::Output<D>>> {
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
        data: snap_meta_cbor_encoded,
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

    export_to_forest_car::<D>(
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
async fn export_to_forest_car<D: Digest>(
    roots: NonEmpty<Cid>,
    prefix_data_frames: Option<Vec<anyhow::Result<ForestCarFrame>>>,
    db: &Arc<impl Blockstore + Send + Sync + 'static>,
    tipset: &Tipset,
    lookup_depth: ChainEpochDelta,
    writer: impl AsyncWrite + Unpin,
    options: Option<ExportOptions>,
) -> anyhow::Result<Option<digest::Output<D>>> {
    let ExportOptions {
        skip_checksum,
        seen,
    } = options.unwrap_or_default();

    let stateroot_lookup_limit = tipset.epoch() - lookup_depth;

    // Wrap writer in optional checksum calculator
    let mut writer = AsyncWriterWithChecksum::<D, _>::new(BufWriter::new(writer), !skip_checksum);

    // Stream stateroots in range (stateroot_lookup_limit+1)..=tipset.epoch(). Also
    // stream all block headers until genesis.
    let blocks = par_buffer(
        // Queue 1k blocks. This is enough to saturate the compressor and blocks
        // are small enough that keeping 1k in memory isn't a problem. Average
        // block size is between 1kb and 2kb.
        1024,
        stream_chain(
            Arc::clone(db),
            tipset.clone().chain_owned(Arc::clone(db)),
            stateroot_lookup_limit,
        )
        .with_seen(seen)
        .track_progress(true),
    );

    // Encode Ipld key-value pairs in zstd frames
    let block_frames = forest::Encoder::compress_stream_default(blocks);
    let frames = futures::stream::iter(prefix_data_frames.unwrap_or_default()).chain(block_frames);

    // Write zstd frames and include a skippable index
    forest::Encoder::write(&mut writer, roots, frames).await?;

    // Flush to ensure everything has been successfully written
    writer.flush().await.context("failed to flush")?;

    let digest = writer.finalize().map_err(|e| Error::Other(e.to_string()))?;

    Ok(digest)
}
