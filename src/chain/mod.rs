// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
pub mod store;
mod weight;
use crate::blocks::Tipset;
use crate::db::car::forest;
use crate::ipld::{stream_chain, CidHashSet};
use crate::utils::io::{AsyncWriterWithChecksum, Checksum};
use anyhow::{Context, Result};
use digest::Digest;
use fvm_ipld_blockstore::Blockstore;
use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter};

pub use self::{store::*, weight::*};

pub async fn export<D: Digest>(
    db: impl Blockstore,
    tipset: &Tipset,
    lookup_depth: ChainEpochDelta,
    writer: impl AsyncWrite + Unpin,
    seen: CidHashSet,
    skip_checksum: bool,
) -> Result<Option<digest::Output<D>>, Error> {
    let stateroot_lookup_limit = tipset.epoch() - lookup_depth;
    let roots = tipset.key().cids().to_vec();

    // Wrap writer in optional checksum calculator
    let mut writer = AsyncWriterWithChecksum::<D, _>::new(BufWriter::new(writer), !skip_checksum);

    // Stream stateroots in range stateroot_lookup_limit..=tipset.epoch(). Also
    // stream all block headers until genesis.
    let blocks =
        stream_chain(&db, tipset.clone().chain(&db), stateroot_lookup_limit).with_seen(seen);

    // Encode Ipld key-value pairs in zstd frames
    let frames = forest::Encoder::compress_stream(8000usize.next_power_of_two(), 3, blocks);

    // Write zstd frames and include a skippable index
    forest::Encoder::write(&mut writer, roots, frames).await?;

    // Flush to ensure everything has been successfully written
    writer.flush().await.context("failed to flush")?;

    let digest = writer.finalize().map_err(|e| Error::Other(e.to_string()))?;

    Ok(digest)
}
