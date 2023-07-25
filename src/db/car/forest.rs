// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// encode CAR-stream into ForestCAR.zst

use crate::db::car::plain::write_skip_frame_header_async;
use crate::utils::db::car_index::{BlockPosition, CarIndex, CarIndexBuilder};
use crate::utils::db::car_stream::{Block, CarHeader};
use crate::utils::encoding::uvibytes::UviBytes;
use ahash::HashMapExt;
use bytes::{buf::Writer, Buf, BufMut as _, Bytes, BytesMut};
use cid::Cid;
use futures::future::Either;
use futures::{Stream, TryStream, TryStreamExt as _};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{from_slice, to_vec};
use parking_lot::Mutex;
use std::sync::Arc;
use std::task::Poll;
use std::{
    io,
    io::{Read, Seek, SeekFrom, Write},
};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio_util::codec::{Decoder, Encoder as _};

pub struct ForestCar<ReaderT> {
    inner: Arc<Mutex<ForestCarInner<ReaderT>>>,
}

struct ForestCarInner<ReaderT> {
    // new_reader: Box<dyn Fn() -> ReaderT>,
    reader: ReaderT,
    write_cache: ahash::HashMap<Cid, Vec<u8>>,
    index: CarIndex<ReaderT>,
    roots: Vec<Cid>,
}

impl<ReaderT: Read + Seek> ForestCar<ReaderT> {
    pub fn open(mk_reader: impl Fn() -> ReaderT + 'static) -> io::Result<Self> {
        let mut reader = mk_reader();

        reader.seek(SeekFrom::End(-(ForestCarFooter::SIZE as i64)))?;
        let mut footer_buffer = [0; ForestCarFooter::SIZE];
        reader.read_exact(&mut footer_buffer)?;
        let footer = ForestCarFooter::try_from_le_bytes(footer_buffer).ok_or(io::Error::new(
            io::ErrorKind::InvalidData,
            "Data not recognized as ForestCAR.zst",
        ))?;

        reader.seek(SeekFrom::Start(0))?;
        let mut header_zstd_frame = decode_zstd_single_frame(&mut reader)?;
        let block_frame = UviBytes::default()
            .decode(&mut header_zstd_frame)?
            .ok_or(invalid_data("malformed uvibytes"))?;
        let header = from_slice::<CarHeader>(&block_frame)?;

        let index = CarIndex::open(mk_reader(), footer.index)?;
        let inner = ForestCarInner {
            // new_reader: Box::new(mk_reader),
            reader,
            write_cache: ahash::HashMap::default(),
            index,
            roots: header.roots,
        };
        Ok(ForestCar {
            inner: Arc::new(Mutex::new(inner)),
        })
    }

    pub fn roots(&self) -> Vec<Cid> {
        self.inner.lock().roots.clone()
    }
}

impl<ReaderT> Blockstore for ForestCar<ReaderT>
where
    ReaderT: Read + Seek,
{
    #[tracing::instrument(level = "trace", skip(self))]
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        let ForestCarInner {
            reader,
            write_cache,
            index,
            ..
        } = &mut *self.inner.lock();
        // Return immediately if the value is cached.
        if let Some(value) = write_cache.get(k) {
            return Ok(Some(value.clone()));
        }

        let stored = index.lookup(*k)?;

        for position in stored.into_iter() {
            // Seek to the start of the zstd frame
            reader.seek(SeekFrom::Start(position.zst_frame_offset()))?;
            // Decode entire frame into memory
            let mut zstd_frame = decode_zstd_single_frame(reader)?;
            // Seek to the start of the block frame
            zstd_frame.advance(position.decoded_offset() as usize);
            // Read block data into memory
            let block_frame = UviBytes::default()
                .decode(&mut zstd_frame)?
                .ok_or(invalid_data("malformed uvibytes"))?;
            // Parse block data as CID+Value pair
            if let Some(block) = Block::from_bytes(block_frame) {
                // This is almost always true. Hash collisions do happen with
                // identity-encoded CIDs, though.
                if block.cid == *k {
                    return Ok(Some(block.data));
                }
            } else {
                return Err(invalid_data("corrupted key-value block"))?;
            }
        }
        return Ok(None);
    }

    #[tracing::instrument(level = "trace", skip(self, block))]
    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        let ForestCarInner { write_cache, .. } = &mut *self.inner.lock();
        debug_assert!(Block {
            cid: *k,
            data: block.to_vec()
        }
        .valid());
        write_cache.insert(*k, Vec::from(block));
        Ok(())
    }
}

fn decode_zstd_single_frame<ReaderT: Read>(reader: &mut ReaderT) -> io::Result<BytesMut> {
    let mut zstd_frame = vec![];
    zstd::Decoder::new(reader)?
        .single_frame()
        .read_to_end(&mut zstd_frame)?;
    // This unnecessarily copies the zstd frame. :(
    Ok(BytesMut::from(zstd_frame.as_slice()))
}

pub struct Encoder {}

impl Encoder {
    pub async fn write(
        sink: &mut (impl AsyncWrite + Unpin),
        roots: Vec<Cid>,
        mut stream: impl TryStream<Ok = Either<(Cid, u16), Bytes>, Error = io::Error> + Unpin,
    ) -> io::Result<()> {
        let mut position = 0;

        // Write CARv1 header
        let mut header_encoder = new_encoder(3)?;

        let header = CarHeader { roots, version: 1 };
        let mut header_uvi_frame = BytesMut::new();
        UviBytes::default().encode(Bytes::from(to_vec(&header)?), &mut header_uvi_frame)?;
        header_encoder.write_all(&header_uvi_frame)?;
        let header_bytes = header_encoder.finish()?.into_inner().freeze();

        sink.write_all(&header_bytes).await?;
        let header_len = header_bytes.len();

        position += header_len;

        // Write seekable zstd and collect a mapping of CIDs to frame_offset+data_offset.
        let mut cid_map = ahash::HashMap::new();
        while let Some(either) = stream.try_next().await? {
            match either {
                Either::Left((cid, offset)) => {
                    cid_map.insert(
                        cid,
                        BlockPosition::new(position as u64, offset).ok_or(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "zstd archive size of 256TiB exceeded",
                        ))?,
                    );
                }
                Either::Right(zstd_frame) => {
                    position += zstd_frame.len();
                    sink.write_all(&zstd_frame).await?;
                }
            }
        }

        // Create index
        let index_offset = position as u64 + 8;
        let builder = CarIndexBuilder::new(cid_map.into_iter());
        write_skip_frame_header_async(sink, builder.encoded_len()).await?;
        builder.write_async(sink).await?;

        // Write ForestCAR.zst footer, it's a valid ZSTD skip-frame
        let footer = ForestCarFooter {
            index: index_offset,
        };
        sink.write_all(&footer.to_le_bytes()).await?;
        Ok(())
    }

    // Consume stream of blocks, emit a new position of each block and a stream
    // of zstd frames.
    pub fn compress_stream(
        zstd_frame_size_tripwire: usize,
        zstd_compression_level: u16,
        stream: impl TryStream<Ok = Block, Error = io::Error>,
    ) -> impl TryStream<Ok = Either<(Cid, u16), Bytes>, Error = io::Error> {
        let mut encoder_store = new_encoder(zstd_compression_level);
        let mut frame_offset: usize = 0;

        let mut stream = Box::pin(stream.into_stream());
        futures::stream::poll_fn(move |cx| {
            let encoder = match encoder_store.as_mut() {
                Err(e) => {
                    let dummy_error =
                        io::Error::new(io::ErrorKind::Other, "Error already consumed.");
                    return Poll::Ready(Some(Err(std::mem::replace(e, dummy_error))));
                }
                Ok(encoder) => encoder,
            };

            // Emit frame if compressed_len >= zstd_frame_size_tripwire OR uncompressed_len >= 2^16
            if compressed_len(encoder) >= zstd_frame_size_tripwire || frame_offset >= 1 << 16 {
                let frame = finalize_frame(zstd_compression_level, encoder)?;
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
                        let frame = finalize_frame(zstd_compression_level, encoder)?;
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
                    block.write(encoder)?;
                    encoder.flush()?;
                    Poll::Ready(Some(Ok(Either::Left((cid, frame_offset_u16)))))
                }
            }
        })
    }
}

fn invalid_data(msg: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
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

struct ForestCarFooter {
    index: u64,
}

impl ForestCarFooter {
    pub const SIZE: usize = 16;

    pub fn to_le_bytes(&self) -> [u8; Self::SIZE] {
        let footer_data_len: u32 = 8;

        let mut buffer = [0; 16];
        buffer[0..4].copy_from_slice(&[0x50, 0x2A, 0x4D, 0x18]);
        buffer[4..8].copy_from_slice(&footer_data_len.to_le_bytes());
        buffer[8..16].copy_from_slice(&self.index.to_le_bytes());
        buffer
    }

    pub fn try_from_le_bytes(bytes: [u8; Self::SIZE]) -> Option<ForestCarFooter> {
        let index = u64::from_le_bytes(bytes[8..16].try_into().expect("infallible"));
        let footer = ForestCarFooter { index };
        if bytes == footer.to_le_bytes() {
            Some(footer)
        } else {
            None
        }
    }
}
