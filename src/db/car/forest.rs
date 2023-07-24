// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// encode CAR-stream into ForestCAR.zst

use crate::car_backed_blockstore::write_skip_frame_async;
use crate::utils::db::car_index::{BlockPosition, CarIndex, CarIndexBuilder};
use crate::utils::db::car_stream::{Block, CarHeader};
use crate::utils::try_finite_stream;
use ahash::HashMapExt;
use bytes::{buf::Writer, BufMut as _, Bytes, BytesMut, Buf};
use cid::Cid;
use futures::future::Either;
use futures::{Stream, TryStream, TryStreamExt as _};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::to_vec;
use num_traits::ToBytes;
use parking_lot::Mutex;
use std::pin::{pin, Pin};
use std::sync::Arc;
use std::task::Poll;
use std::{
    io,
    io::{Cursor, Read, Seek, SeekFrom, Write},
};
use tokio::io::{AsyncRead, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt};

// Input: stream of blocks
// Output: (BlockPosition, Option<zst_frame>)

pub struct ForestCAR<ReaderT> {
    inner: Arc<Mutex<ForestCARInner<ReaderT>>>,
}

struct ForestCARInner<ReaderT> {
    new_reader: Box<dyn Fn() -> ReaderT>,
    reader: ReaderT,
    write_cache: ahash::HashMap<Cid, Vec<u8>>,
    index: CarIndex<ReaderT>,
    roots: Vec<Cid>,
}

impl<ReaderT: Read + Seek> ForestCAR<ReaderT> {
    pub fn open(mk_reader: impl Fn() -> ReaderT + 'static) -> io::Result<Self> {
        let mut reader = mk_reader();

        reader.seek(SeekFrom::End(-(ForestCARFooter::SIZE as i64)))?;
        let mut footer_buffer = [0; ForestCARFooter::SIZE];
        reader.read_exact(&mut footer_buffer);
        let footer = ForestCARFooter::try_from_le_bytes(footer_buffer).ok_or(io::Error::new(
            io::ErrorKind::InvalidData,
            "Data not recognized as ForestCAR.zst",
        ))?;

        let index = CarIndex::open(mk_reader(), footer.index)?;
        let inner = ForestCARInner {
            new_reader: Box::new(mk_reader),
            reader,
            write_cache: ahash::HashMap::default(),
            index,
            roots: Vec::default(),
        };
        Ok(ForestCAR {
            inner: Arc::new(Mutex::new(inner)),
        })
    }
}

impl<ReaderT> Blockstore for ForestCAR<ReaderT>
where
    ReaderT: Read + Seek,
{
    #[tracing::instrument(level = "trace", skip(self))]
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        let ForestCARInner {
            reader,
            write_cache,
            index,
            ..
        } = &mut *self.inner.lock();
        if let Some(value) = write_cache.get(k) {
            return Ok(Some(value.clone()))
        }

        let stored = index.lookup(*k)?;
        for position in stored.into_iter() {
            reader.seek(SeekFrom::Start(position.zst_frame_offset()))?;
            let mut zstd_frame = std::io::Cursor::new(vec![]);
            zstd::Decoder::new(reader.by_ref())
                .expect("we're not using a custom dictionary")
                .single_frame()
                .read_to_end(zstd_frame.get_mut())?;
            let mut bytes = Bytes::from(zstd_frame.into_inner());
            bytes.advance(position.decoded_offset() as usize);
            if let Some(block) = Block::from_bytes(bytes) {
                if block.cid == *k {
                    return Ok(Some(block.data))
                }
            }
        }
        return Ok(None)
    }

    /// # Panics
    /// - If the write cache contains different data with this CID
    /// - See also [`Self::new`].
    #[tracing::instrument(level = "trace", skip(self, block))]
    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        let ForestCARInner {
            write_cache, index, ..
        } = &mut *self.inner.lock();
        write_cache.insert(*k, Vec::from(block));
        Ok(())
    }
}

pub struct Encoder {}

impl Encoder {
    pub async fn write(
        sink: &mut (impl AsyncWrite + AsyncSeek + Unpin),
        roots: Vec<Cid>,
        mut stream: impl TryStream<Ok = Either<(Cid, BlockPosition), Bytes>, Error = io::Error> + Unpin,
    ) -> io::Result<()> {
        // Write CARv1 header
        let header = CarHeader { roots, version: 1 };
        sink.write_all(&to_vec(&header)?).await?;

        // Write seekable zstd and collect a mapping of CIDs to frame_offset+data_offset.
        let mut cid_map = ahash::HashMap::new();
        while let Some(either) = stream.try_next().await? {
            match either {
                Either::Left((cid, position)) => {
                    cid_map.insert(cid, position);
                }
                Either::Right(zstd_frame) => {
                    sink.write_all(&zstd_frame).await?;
                }
            }
        }

        // Create index
        let index_offset = sink.stream_position().await?;
        let mut index = Vec::new();
        CarIndexBuilder::new(cid_map.into_iter()).write(Cursor::new(&mut index))?;
        write_skip_frame_async(sink, &index).await?;

        // Write ForestCAR.zst footer
        let footer = ForestCARFooter {
            index: index_offset,
        };
        write_skip_frame_async(sink, &footer.to_le_bytes()).await?;
        Ok(())
    }

    // Consume stream of blocks, emit a new position of each block and a stream
    // of zstd frames.
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
                    frame_offset += block.encoded_len();
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

struct ForestCARFooter {
    index: u64,
}

impl ForestCARFooter {
    pub const SIZE: usize = 16;

    pub fn to_le_bytes(&self) -> [u8; Self::SIZE] {
        let footer_data_len: u32 = 8;

        let mut buffer = [0; 16];
        buffer[0..4].copy_from_slice(&[0x50, 0x2A, 0x4D, 0x18]);
        buffer[4..8].copy_from_slice(&footer_data_len.to_le_bytes());
        buffer[8..16].copy_from_slice(&self.index.to_le_bytes());
        buffer
    }

    pub fn try_from_le_bytes(bytes: [u8; Self::SIZE]) -> Option<ForestCARFooter> {
        let index = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let footer = ForestCARFooter { index };
        if bytes == footer.to_le_bytes() {
            Some(footer)
        } else {
            None
        }
    }
}
