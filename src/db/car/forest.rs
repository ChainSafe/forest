// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// encode CAR-stream into ForestCAR.zst

use crate::utils::db::car_index::BlockPosition;
use crate::utils::db::car_stream::{Block, CarHeader};
use crate::utils::try_finite_stream;
use bytes::{buf::Writer, BufMut as _, Bytes, BytesMut};
use cid::Cid;
use futures::future::Either;
use futures::{Stream, TryStream, TryStreamExt as _};
use std::task::Poll;
use std::{io, io::Write};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use std::pin::{Pin, pin};
use ahash::HashMapExt;

// Input: stream of blocks
// Output: (BlockPosition, Option<zst_frame>)

struct ForestCAR {}

impl ForestCAR {
    pub async fn write(
        sink: &mut (impl AsyncWrite + Unpin),
        mut stream: impl TryStream<Ok = Either<(Cid, BlockPosition), Bytes>, Error = io::Error> + Unpin,
    ) -> io::Result<()> {
        let mut cid_map = ahash::HashMap::new();
        // Write seekable zstd and collect a mapping of CIDs to frame_offset+data_offset.
        while let Some(either) = stream.try_next().await? {
            match either {
                Either::Left((cid, position)) => {
                    cid_map.insert(cid, position);
                },
                Either::Right(zstd_frame) => {
                    sink.write_all(&zstd_frame).await?;
                }
            }
        }
        // Create index
        // crate::car_backed_blockstore::write_skip_frame(&mut file, &index)?;
        Ok(())
    }

    pub fn compress_stream(
        zstd_frame_size_tripwire: usize,
        zstd_compression_level: u16,
        stream: impl TryStream<Ok = Block, Error = io::Error>,
    ) -> impl TryStream<Ok = Either<(Cid, BlockPosition), Bytes>, Error = io::Error> {
        let mut encoder = new_encoder(zstd_compression_level).unwrap();
        let mut emitted_bytes: usize = 0;
        let mut frame_offset: usize = 0;

        let mut stream = Box::pin(stream.into_stream());
        futures::stream::poll_fn(move |cx| {
            // Emit frame if compressed_len >= zstd_frame_size_tripwire OR uncompressed_len >= 2^16
            if compressed_len(&encoder) >= zstd_frame_size_tripwire || frame_offset >= 1 << 16 {
                let frame = finalize_frame(zstd_compression_level, &mut encoder)?;
                emitted_bytes += frame.len();
                frame_offset = 0;
                return Poll::Ready(Some(Ok(Either::Right(frame))));
            }
            // No frame to emit, let's get another block
            let ret = futures::ready!(stream.as_mut().poll_next(cx));
            match ret {
                // End-of-stream
                None => {
                    // If there's anything in the zstd buffer, emit it.
                    if compressed_len(&encoder) > 0 {
                        let frame = finalize_frame(zstd_compression_level, &mut encoder)?;
                        Poll::Ready(Some(Ok(Either::Right(frame))))
                    } else {
                        // Otherwise we're all done.
                        Poll::Ready(None)
                    }
                }
                // Pass errors through
                Some(Err(e)) => Poll::Ready(Some(Err(e))),
                // Got element, add to encoder and emit block position
                Some(Ok(block)) => {
                    let cid = block.cid;
                    let frame_offset_u16 =
                        u16::try_from(frame_offset).ok().ok_or(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "frame_offset should fit in 16 bits",
                        ))?;
                    let position = BlockPosition::new(emitted_bytes as u64, frame_offset_u16)
                        .ok_or(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "zstd archive size of 256TiB exceeded",
                        ))?;
                    block.write(&mut encoder)?;
                    encoder.flush()?;
                    Poll::Ready(Some(Ok(Either::Left((cid, position)))))
                }
            }
        })
    }
}

fn compressed_len(encoder: &zstd::Encoder<'static, Writer<BytesMut>>) -> usize {
    encoder.get_ref().get_ref().len()
}

fn finalize_frame(
    zstd_compression_level: u16,
    encoder: &mut zstd::Encoder<'static, Writer<BytesMut>>,
) -> io::Result<Bytes> {
    let prev_encoder = std::mem::replace(encoder, new_encoder(zstd_compression_level)?);
    Ok(prev_encoder.finish()?.into_inner().freeze())
}

fn new_encoder(
    zstd_compression_level: u16,
) -> io::Result<zstd::Encoder<'static, Writer<BytesMut>>> {
    zstd::Encoder::new(BytesMut::new().writer(), i32::from(zstd_compression_level))
}
