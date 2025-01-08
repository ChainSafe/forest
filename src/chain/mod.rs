// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
pub mod store;
mod weight;
use crate::blocks::Tipset;
use crate::cid_collections::CidHashSet;
use crate::db::car::forest;
use crate::ipld::stream_chain;
use crate::utils::io::{AsyncWriterWithChecksum, Checksum};
use crate::utils::stream::par_buffer;
use anyhow::Context as _;
use digest::Digest;
use fvm_ipld_blockstore::Blockstore;
use std::sync::Arc;
use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter};

pub use self::{store::*, weight::*};

pub async fn export<D: Digest>(
    db: Arc<impl Blockstore + Send + Sync + 'static>,
    tipset: &Tipset,
    lookup_depth: ChainEpochDelta,
    writer: impl AsyncWrite + Unpin,
    seen: CidHashSet,
    skip_checksum: bool,
) -> anyhow::Result<Option<digest::Output<D>>, Error> {
    let stateroot_lookup_limit = tipset.epoch() - lookup_depth;
    let roots = tipset.key().to_cids();

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
            Arc::clone(&db),
            tipset.clone().chain_owned(Arc::clone(&db)),
            stateroot_lookup_limit,
        )
        .with_seen(seen),
    );

    // Encode Ipld key-value pairs in zstd frames
    let frames = forest::Encoder::compress_stream_default(blocks);

    // Write zstd frames and include a skippable index
    forest::Encoder::write(&mut writer, roots, frames).await?;

    // Flush to ensure everything has been successfully written
    writer.flush().await.context("failed to flush")?;

    let digest = writer.finalize().map_err(|e| Error::Other(e.to_string()))?;

    Ok(digest)
}
