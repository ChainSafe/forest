// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
pub mod store;
mod weight;
use crate::blocks::Tipset;
use crate::ipld::stream_chain;
use crate::utils::io::{AsyncWriterWithChecksum, Checksum};
use anyhow::{Context, Result};
use async_compression::futures::write::ZstdEncoder;
use digest::Digest;
use futures::{io::BufWriter, AsyncWrite};
use futures_util::future::Either;
use futures_util::AsyncWriteExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_car::CarHeader;
use std::sync::Arc;

pub use self::{store::*, weight::*};

pub async fn export<W, D>(
    db: impl Blockstore + Send + Sync,
    tipset: &Tipset,
    lookup_depth: ChainEpochDelta,
    writer: W,
    compressed: bool,
    skip_checksum: bool,
) -> Result<Option<digest::Output<D>>, Error>
where
    D: Digest + Send + 'static,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let store = Arc::new(db);
    use futures::StreamExt;
    let writer = AsyncWriterWithChecksum::<D, _>::new(BufWriter::new(writer), !skip_checksum);
    let mut writer = if compressed {
        Either::Left(ZstdEncoder::new(writer))
    } else {
        Either::Right(writer)
    };

    let stateroot_lookup_limit = tipset.epoch() - lookup_depth;

    let mut stream = stream_chain(&store, tipset.clone().chain(&store), stateroot_lookup_limit)
        .map(|result| result.unwrap()); // FIXME: use a sink that supports TryStream.
    let header = CarHeader::from(tipset.key().cids().to_vec());
    header
        .write_stream_async(&mut writer, &mut stream)
        .await
        .map_err(|e| Error::Other(format!("Failed to write blocks in export: {e}")))?;

    writer.flush().await.context("failed to flush")?;
    writer.close().await.context("failed to close")?;

    let digest = match &mut writer {
        Either::Left(left) => left.get_mut().finalize().await,
        Either::Right(right) => right.finalize().await,
    }
    .map_err(|e| Error::Other(e.to_string()))?;

    Ok(digest)
}
