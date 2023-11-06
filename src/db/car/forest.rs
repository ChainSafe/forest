// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! # Forest CAR format
//!
//! See [`crate::db::car::plain`] for details on the CAR format.
//!
//! The `forest.car.zst` format wraps multiple CAR blocks in small (usually 8 KiB)
//! zstd frames, and has an index in a skippable zstd frame. At the end of the
//! data, there has to be a fixed-size skippable frame containing magic numbers
//! and meta information about the archive. CAR blocks may not span multiple
//! z-frames and the CAR header is kept it a separate z-frame.
//!
//! Imagine a `forest.car.zst` archive with 5 blocks. They could be arranged in
//! z-frames as drawn below:
//!
//! ```text
//!  Z-Frame 1   Z-Frame 2   Z-Frame 3   Skip Frame    Skip Frame
//! ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌───────────┐ ┌────────────┐
//! │┌──────┐ │ │┌───────┐│ │┌───────┐│ │Offsets    │ │Index offset│
//! ││Header│ │ ││Block 1││ ││Block 4││ │ Z-Frame 2 │ │Magic number│
//! │└──────┘ │ │└───────┘│ │└───────┘│ │ Z-Frame 2 │ │Version info|
//! └─────────┘ │┌───────┐│ │┌───────┐│ │ Z-Frame 2 │ └────────────┘
//!             ││Block 2││ ││Block 5││ │ Z-Frame 3 │
//!             │└───────┘│ │└───────┘│ │ Z-Frame 3 │
//!             │┌───────┐│ └─────────┘ └───────────┘
//!             ││Block 3││
//!             │└───────┘│
//!             └─────────┘
//! ```
//!
//! Looking up a block uses an [`index::Reader`] to find
//! the right z-frame. The frame is then decoded and each block is linearly
//! scanned until a match is found. Decoded (and scanned) z-frames are stored in
//! a lru-cache for faster repeat retrievals.
//!
//! `forest.car.zst` files are backward compatible with Lotus (and all other
//! tools that consume compressed CAR files). All Forest-specifc information is
//! encoded as skippable frames that are (as the name suggests) skipped by tools
//! that don't understand them.
//!
//! # Additional reading
//!
//! `zstd` frame format: <https://github.com/facebook/zstd/blob/dev/doc/zstd_compression_format.md>
//!
//! CARv1 specification: <https://ipld.io/specs/transport/car/carv1/>
//!

use super::{CacheKey, ZstdFrameCache};
use crate::blocks::{Tipset, TipsetKeys};
use crate::cid_collections::CidHashMap;
use crate::db::car::plain::write_skip_frame_header_async;
use crate::db::car::RandomAccessFileReader;
use crate::utils::db::car_stream::{CarBlock, CarHeader};
use crate::utils::encoding::from_slice_with_fallback;
use crate::utils::io::EitherMmapOrRandomAccessFile;
use ahash::{HashMap, HashMapExt};
use bytes::{buf::Writer, BufMut as _, Bytes, BytesMut};
use cid::Cid;
use futures::{Stream, TryStream, TryStreamExt as _};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::to_vec;
use parking_lot::{Mutex, RwLock};
use positioned_io::{Cursor, ReadAt, SizeCursor};
use std::io::{Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;
use std::task::Poll;
use std::{
    io,
    io::{Read, Write},
};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio_util::codec::{Decoder, Encoder as _};
use unsigned_varint::codec::UviBytes;
#[cfg(feature = "benchmark-private")]
pub mod index;
#[cfg(not(feature = "benchmark-private"))]
mod index;

pub const FOREST_CAR_FILE_EXTENSION: &str = ".forest.car.zst";
pub const DEFAULT_FOREST_CAR_FRAME_SIZE: usize = 8000_usize.next_power_of_two();
pub const DEFAULT_FOREST_CAR_COMPRESSION_LEVEL: u16 = zstd::DEFAULT_COMPRESSION_LEVEL as _;

pub trait ReaderGen<V>: Fn() -> io::Result<V> + Send + Sync + 'static {}
impl<ReaderT, X: Fn() -> io::Result<ReaderT> + Send + Sync + 'static> ReaderGen<ReaderT> for X {}

pub struct ForestCar<ReaderT> {
    // Multiple `ForestCar` structures may share the same cache. The cache key is used to identify
    // the origin of a cached z-frame.
    cache_key: CacheKey,
    indexed: index::Reader<positioned_io::Slice<ReaderT>>,
    frame_cache: Arc<Mutex<ZstdFrameCache>>,
    write_cache: Arc<RwLock<ahash::HashMap<Cid, Vec<u8>>>>,
    roots: Vec<Cid>,
}

impl<ReaderT: super::RandomAccessFileReader> ForestCar<ReaderT> {
    pub fn new(reader: ReaderT) -> io::Result<ForestCar<ReaderT>> {
        let (header, footer) = Self::validate_car(&reader)?;

        let indexed = index::Reader::new(positioned_io::Slice::new(reader, footer.index, None))?;

        Ok(ForestCar {
            cache_key: 0,
            indexed,
            frame_cache: Arc::new(Mutex::new(ZstdFrameCache::default())),
            write_cache: Arc::new(RwLock::new(ahash::HashMap::default())),
            roots: header.roots,
        })
    }

    pub fn is_valid(reader: &ReaderT) -> bool {
        Self::validate_car(reader).is_ok()
    }

    fn validate_car(reader: &ReaderT) -> io::Result<(CarHeader, ForestCarFooter)> {
        let mut cursor = SizeCursor::new(&reader);
        cursor.seek(SeekFrom::End(-(ForestCarFooter::SIZE as i64)))?;

        let mut footer_buffer = [0; ForestCarFooter::SIZE];
        cursor.read_exact(&mut footer_buffer)?;

        let footer = ForestCarFooter::try_from_le_bytes(footer_buffer).ok_or_else(|| {
            invalid_data(format!(
                "not recognizable as a `{}` file",
                FOREST_CAR_FILE_EXTENSION
            ))
        })?;

        let cursor = Cursor::new_pos(&reader, 0);
        let mut header_zstd_frame = decode_zstd_single_frame(cursor)?;
        let block_frame = UviBytes::<Bytes>::default()
            .decode(&mut header_zstd_frame)?
            .ok_or_else(|| invalid_data("malformed uvibytes"))?;
        let header = from_slice_with_fallback::<CarHeader>(&block_frame)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        Ok((header, footer))
    }

    pub fn roots(&self) -> Vec<Cid> {
        self.roots.clone()
    }

    pub fn heaviest_tipset(&self) -> anyhow::Result<Tipset> {
        Tipset::load_required(self, &TipsetKeys::from_iter(self.roots()))
    }

    pub fn into_dyn(self) -> ForestCar<Box<dyn super::RandomAccessFileReader>> {
        ForestCar {
            cache_key: self.cache_key,
            indexed: self.indexed.map(|slice| {
                let offset = slice.offset();
                positioned_io::Slice::new(
                    Box::new(slice.into_inner()) as Box<dyn RandomAccessFileReader>,
                    offset,
                    None,
                )
            }),
            frame_cache: self.frame_cache,
            write_cache: self.write_cache,
            roots: self.roots,
        }
    }

    pub fn with_cache(self, cache: Arc<Mutex<ZstdFrameCache>>, key: CacheKey) -> Self {
        Self {
            cache_key: key,
            frame_cache: cache,
            ..self
        }
    }
}

impl TryFrom<&Path> for ForestCar<EitherMmapOrRandomAccessFile> {
    type Error = std::io::Error;
    fn try_from(path: &Path) -> std::io::Result<Self> {
        ForestCar::new(EitherMmapOrRandomAccessFile::open(path)?)
    }
}

impl<ReaderT> Blockstore for ForestCar<ReaderT>
where
    ReaderT: ReadAt,
{
    #[tracing::instrument(level = "trace", skip(self))]
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        // Return immediately if the value is cached.
        if let Some(value) = self.write_cache.read().get(k) {
            return Ok(Some(value.clone()));
        }

        let indexed = &self.indexed;
        for position in indexed.get(*k)?.into_iter() {
            let reader = indexed.reader();
            let cache_query = self.frame_cache.lock().get(position, self.cache_key, *k);
            match cache_query {
                // Frame cache hit, found value.
                Some(Some(val)) => return Ok(Some(val)),
                // Frame cache hit, no value. This only happens when hashes collide
                Some(None) => {}
                None => {
                    // Decode entire frame into memory, "position" arg is the frame start offset.
                    let cursor = Cursor::new_pos(reader, position);
                    let mut zstd_frame = decode_zstd_single_frame(cursor)?;
                    // Parse all key-value pairs and insert them into a map
                    let mut block_map = HashMap::new();
                    while let Some(block_frame) =
                        UviBytes::<Bytes>::default().decode_eof(&mut zstd_frame)?
                    {
                        let CarBlock { cid, data } = CarBlock::from_bytes(block_frame)?;
                        block_map.insert(cid, data);
                    }
                    let get_result = block_map.get(k).cloned();
                    self.frame_cache
                        .lock()
                        .put(position, self.cache_key, block_map);

                    // This lookup only fails in case of a hash collision
                    if let Some(value) = get_result {
                        return Ok(Some(value));
                    }
                }
            }
        }
        Ok(None)
    }

    #[tracing::instrument(level = "trace", skip(self, block))]
    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        debug_assert!(CarBlock {
            cid: *k,
            data: block.to_vec()
        }
        .valid());
        self.write_cache.write().insert(*k, Vec::from(block));
        Ok(())
    }
}

fn decode_zstd_single_frame<ReaderT: Read>(reader: ReaderT) -> io::Result<BytesMut> {
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
        mut stream: impl TryStream<Ok = (Vec<Cid>, Bytes), Error = anyhow::Error> + Unpin,
    ) -> anyhow::Result<()> {
        let mut offset = 0;

        // Write CARv1 header
        let mut header_encoder = new_encoder(3)?;

        let header = CarHeader { roots, version: 1 };
        let mut header_uvi_frame = BytesMut::new();
        UviBytes::default().encode(Bytes::from(to_vec(&header)?), &mut header_uvi_frame)?;
        header_encoder.write_all(&header_uvi_frame)?;
        let header_bytes = header_encoder.finish()?.into_inner().freeze();

        sink.write_all(&header_bytes).await?;
        let header_len = header_bytes.len();

        offset += header_len;

        // Write seekable zstd and collect a mapping of CIDs to frame_offset+data_offset.
        let mut cid_to_frame_offset = CidHashMap::new();
        while let Some((cids, zstd_frame)) = stream.try_next().await? {
            cid_to_frame_offset.extend(cids.into_iter().map(|cid| (cid, offset as u64)));
            sink.write_all(&zstd_frame).await?;
            offset += zstd_frame.len()
        }

        // Create index
        let mut index = vec![];
        index::write(cid_to_frame_offset, &mut index).expect("Vec has infallible IO");
        write_skip_frame_header_async(sink, index.len().try_into().unwrap()).await?;
        sink.write_all(&index).await?;

        // Write ForestCAR.zst footer, it's a valid ZSTD skip-frame
        let footer = ForestCarFooter {
            // TODO(aatifsyed): why the addition?
            index: offset as u64 + 8,
        };
        sink.write_all(&footer.to_le_bytes()).await?;
        Ok(())
    }

    /// `compress_stream` with [`DEFAULT_FOREST_CAR_FRAME_SIZE`] as default frame size and [`DEFAULT_FOREST_CAR_COMPRESSION_LEVEL`] as default compression level.
    pub fn compress_stream_default(
        stream: impl TryStream<Ok = CarBlock, Error = anyhow::Error>,
    ) -> impl TryStream<Ok = (Vec<Cid>, Bytes), Error = anyhow::Error> {
        Self::compress_stream(
            DEFAULT_FOREST_CAR_FRAME_SIZE,
            DEFAULT_FOREST_CAR_COMPRESSION_LEVEL,
            stream,
        )
    }

    /// Consume stream of blocks, emit a new position of each block and a stream
    /// of zstd frames.
    pub fn compress_stream(
        zstd_frame_size_tripwire: usize,
        zstd_compression_level: u16,
        stream: impl TryStream<Ok = CarBlock, Error = anyhow::Error>,
    ) -> impl TryStream<Ok = (Vec<Cid>, Bytes), Error = anyhow::Error> {
        let mut encoder_store = new_encoder(zstd_compression_level);
        let mut frame_cids = vec![];

        let mut stream = Box::pin(stream.into_stream());
        futures::stream::poll_fn(move |cx| {
            let encoder = match encoder_store.as_mut() {
                Err(e) => {
                    let dummy_error = io::Error::other("Error already consumed.");
                    return Poll::Ready(Some(Err(anyhow::Error::from(std::mem::replace(
                        e,
                        dummy_error,
                    )))));
                }
                Ok(encoder) => encoder,
            };
            loop {
                // Emit frame if compressed_len > zstd_frame_size_tripwire
                if compressed_len(encoder) > zstd_frame_size_tripwire {
                    let cids = std::mem::take(&mut frame_cids);
                    let frame = finalize_frame(zstd_compression_level, encoder)?;
                    return Poll::Ready(Some(Ok((cids, frame))));
                }
                // No frame to emit, let's get another block
                let ret = futures::ready!(stream.as_mut().poll_next(cx));
                match ret {
                    // End-of-stream
                    None => {
                        // If there's anything in the zstd buffer, emit it.
                        if compressed_len(encoder) > 0 {
                            let cids = std::mem::take(&mut frame_cids);
                            let frame = finalize_frame(zstd_compression_level, encoder)?;
                            return Poll::Ready(Some(Ok((cids, frame))));
                        } else {
                            // Otherwise we're all done.
                            return Poll::Ready(None);
                        }
                    }
                    // Pass errors through
                    Some(Err(e)) => return Poll::Ready(Some(Err(e))),
                    // Got element, add to encoder and emit block position
                    Some(Ok(block)) => {
                        frame_cids.push(block.cid);
                        block.write(encoder)?;
                        encoder.flush()?;
                    }
                }
            }
        })
    }
}

fn invalid_data(inner: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, inner)
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

#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
struct ForestCarFooter {
    index: u64,
}

impl ForestCarFooter {
    pub const SIZE: usize = 16;

    pub fn to_le_bytes(&self) -> [u8; Self::SIZE] {
        let footer_data_len: u32 = 8;

        let mut buffer = [0; 16];
        // Skippable frames start with 50 2A 4D 18
        buffer[0..4].copy_from_slice(&[0x50, 0x2A, 0x4D, 0x18]);
        // Then a u32 containing the length of the data in the frame
        buffer[4..8].copy_from_slice(&footer_data_len.to_le_bytes());
        // And finally the metadata we want to store
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use quickcheck_macros::quickcheck;

    fn mk_encoded_car(
        zstd_frame_size_tripwire: usize,
        zstd_compression_level: u16,
        roots: Vec<Cid>,
        block: Vec<CarBlock>,
    ) -> Vec<u8> {
        block_on(async {
            let frame_stream = Encoder::compress_stream(
                zstd_frame_size_tripwire,
                zstd_compression_level,
                futures::stream::iter(block.into_iter().map(Ok)),
            );
            let mut encoded = vec![];
            Encoder::write(&mut encoded, roots, frame_stream)
                .await
                .unwrap();
            encoded
        })
    }

    #[quickcheck]
    fn forest_car_create_basic(head: CarBlock, mut tail: Vec<CarBlock>, roots: Vec<Cid>) {
        tail.push(head);
        let forest_car =
            ForestCar::new(mk_encoded_car(1024 * 4, 3, roots.clone(), tail.clone())).unwrap();
        assert_eq!(forest_car.roots(), roots);
        for block in tail {
            assert_eq!(forest_car.get(&block.cid).unwrap(), Some(block.data));
        }
    }

    #[quickcheck]
    fn forest_car_create_options(
        head: CarBlock,
        mut tail: Vec<CarBlock>,
        roots: Vec<Cid>,
        frame_size: usize,
        mut compression_level: u16,
    ) {
        compression_level %= 15;
        tail.push(head);

        let forest_car = ForestCar::new(mk_encoded_car(
            frame_size,
            compression_level.max(1),
            roots.clone(),
            tail.clone(),
        ))
        .unwrap();
        assert_eq!(forest_car.roots(), roots);
        for block in tail {
            assert_eq!(forest_car.get(&block.cid).unwrap(), Some(block.data));
        }
    }

    #[quickcheck]
    fn forest_car_open_invalid(junk: Vec<u8>) {
        // The chance of thinking random data is a valid ForestCar should be practically zero.
        assert!(ForestCar::new(junk).is_err());
    }

    #[quickcheck]
    fn forest_footer_roundtrip(footer: ForestCarFooter) {
        let footer_recoded = ForestCarFooter::try_from_le_bytes(footer.to_le_bytes());
        assert_eq!(footer_recoded, Some(footer));
    }

    // Two colliding hashes in separate zstd-frames should not affect each other.
    #[test]
    fn encode_hash_collisions() {
        use cid::multihash::{Code::Identity, MultihashDigest};

        // Distinct CIDs may map to the same hash value
        let cid_a = Cid::new_v1(0, Identity.digest(&[10]));
        let cid_b = Cid::new_v1(0, Identity.digest(&[0]));
        // A and B are _not_ the same...
        assert_ne!(cid_a, cid_b);
        // ... but they map to the same hash:
        assert_eq!(index::hash::of(&cid_a), index::hash::of(&cid_b));

        // For testing purposes, we ignore that the data doesn't map to the
        // CIDs.
        let blocks = vec![
            CarBlock {
                cid: cid_a,
                data: Vec::from_iter(*b"bill and ben"),
            },
            CarBlock {
                cid: cid_b,
                data: Vec::from_iter(*b"the flowerpot men"),
            },
        ];

        // Setting the desired frame size to 0 means each block will be put in a separate frame.
        let forest_car = ForestCar::new(mk_encoded_car(0, 3, Vec::new(), blocks.clone())).unwrap();

        // Even with colliding hashes, the CIDs can still be queried:
        assert_eq!(forest_car.get(&cid_a).unwrap().unwrap(), blocks[0].data);
        assert_eq!(forest_car.get(&cid_b).unwrap().unwrap(), blocks[1].data);
    }
}
