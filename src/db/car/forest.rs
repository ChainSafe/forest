// Copyright 2019-2025 ChainSafe Systems
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
use crate::blocks::{Tipset, TipsetKey};
use crate::chain::FilecoinSnapshotMetadata;
use crate::db::car::RandomAccessFileReader;
use crate::db::car::plain::write_skip_frame_header_async;
use crate::utils::db::car_stream::{CarBlock, CarV1Header, uvi_bytes};
use crate::utils::encoding::from_slice_with_fallback;
use crate::utils::get_size::CidWrapper;
use crate::utils::io::EitherMmapOrRandomAccessFile;
use byteorder::LittleEndian;
use bytes::{BufMut as _, Bytes, BytesMut, buf::Writer};
use cid::Cid;
use futures::{Stream, TryStreamExt as _};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore as _;
use integer_encoding::VarIntReader;
use nunny::Vec as NonEmpty;
use positioned_io::{Cursor, ReadAt, ReadBytesAtExt, SizeCursor};
use std::io::{Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::task::Poll;
use std::{
    io,
    io::{Read, Write},
};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio_util::codec::{Decoder, Encoder as _};

#[cfg(feature = "benchmark-private")]
pub mod index;
#[cfg(not(feature = "benchmark-private"))]
mod index;

pub const FOREST_CAR_FILE_EXTENSION: &str = ".forest.car.zst";
pub const TEMP_FOREST_CAR_FILE_EXTENSION: &str = ".forest.car.zst.tmp";
/// <https://github.com/facebook/zstd/blob/dev/doc/zstd_compression_format.md#skippable-frames>
pub const ZSTD_SKIPPABLE_FRAME_MAGIC_HEADER: [u8; 4] = [0x50, 0x2A, 0x4D, 0x18];
pub const DEFAULT_FOREST_CAR_FRAME_SIZE: usize = 8000_usize.next_power_of_two();
pub const DEFAULT_FOREST_CAR_COMPRESSION_LEVEL: u16 = zstd::DEFAULT_COMPRESSION_LEVEL as _;
const ZSTD_SKIP_FRAME_LEN: u64 = 8;

/// `zstd` frame of Forest CAR
pub type ForestCarFrame = (Vec<Cid>, Bytes);

pub struct ForestCar<ReaderT> {
    // Multiple `ForestCar` structures may share the same cache. The cache key is used to identify
    // the origin of a cached z-frame.
    cache_key: CacheKey,
    indexed: index::Reader<positioned_io::Slice<ReaderT>>,
    index_size_bytes: u32,
    frame_cache: Arc<ZstdFrameCache>,
    header: CarV1Header,
    metadata: OnceLock<Option<FilecoinSnapshotMetadata>>,
}

impl<ReaderT: super::RandomAccessFileReader> ForestCar<ReaderT> {
    pub fn new(reader: ReaderT) -> io::Result<ForestCar<ReaderT>> {
        let (header, footer) = Self::validate_car(&reader)?;
        let index_size_bytes = reader.read_u32_at::<LittleEndian>(
            footer.index.saturating_sub(std::mem::size_of::<u32>() as _),
        )?;
        let indexed = index::Reader::new(positioned_io::Slice::new(
            reader,
            footer.index,
            Some(index_size_bytes as u64),
        ))?;
        Ok(ForestCar {
            cache_key: 0,
            indexed,
            index_size_bytes,
            frame_cache: Arc::new(ZstdFrameCache::default()),
            header,
            metadata: OnceLock::new(),
        })
    }

    pub fn metadata(&self) -> &Option<FilecoinSnapshotMetadata> {
        self.metadata.get_or_init(|| {
            if self.header.roots.len() == super::V2_SNAPSHOT_ROOT_COUNT {
                let maybe_metadata_cid = self.header.roots.first();
                if let Ok(Some(metadata)) =
                    self.get_cbor::<FilecoinSnapshotMetadata>(maybe_metadata_cid)
                {
                    return Some(metadata);
                }
            }
            None
        })
    }

    pub fn is_valid(reader: &ReaderT) -> bool {
        Self::validate_car(reader).is_ok()
    }

    fn validate_car(reader: &ReaderT) -> io::Result<(CarV1Header, ForestCarFooter)> {
        let mut cursor = SizeCursor::new(&reader);
        cursor.seek(SeekFrom::End(-(ForestCarFooter::SIZE as i64)))?;

        let mut footer_buffer = [0; ForestCarFooter::SIZE];
        cursor.read_exact(&mut footer_buffer)?;

        let footer = ForestCarFooter::try_from_le_bytes(footer_buffer).ok_or_else(|| {
            invalid_data(format!(
                "not recognizable as a `{FOREST_CAR_FILE_EXTENSION}` file"
            ))
        })?;

        let cursor = Cursor::new_pos(&reader, 0);
        let mut header_zstd_frame = decode_zstd_single_frame(cursor)?;
        let block_frame = uvi_bytes()
            .decode(&mut header_zstd_frame)?
            .ok_or_else(|| invalid_data("malformed uvibytes"))?;
        let header = from_slice_with_fallback::<CarV1Header>(&block_frame)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        Ok((header, footer))
    }

    pub fn head_tipset_key(&self) -> &NonEmpty<Cid> {
        // head tipset key is stored in v2 snapshot metadata
        // See <https://github.com/filecoin-project/FIPs/blob/98e33b9fa306959aa0131519eb4cc155522b2081/FRCs/frc-0108.md#v2-specification>
        if let Some(metadata) = self.metadata() {
            &metadata.head_tipset_key
        } else {
            &self.header.roots
        }
    }

    pub fn index_size_bytes(&self) -> u32 {
        self.index_size_bytes
    }

    pub fn heaviest_tipset_key(&self) -> TipsetKey {
        TipsetKey::from(self.head_tipset_key().clone())
    }

    pub fn heaviest_tipset(&self) -> anyhow::Result<Tipset> {
        Tipset::load_required(self, &self.heaviest_tipset_key())
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
            index_size_bytes: self.index_size_bytes,
            frame_cache: self.frame_cache,
            header: self.header,
            metadata: self.metadata,
        }
    }

    pub fn with_cache(self, cache: Arc<ZstdFrameCache>, key: CacheKey) -> Self {
        Self {
            cache_key: key,
            frame_cache: cache,
            ..self
        }
    }

    /// Gets a reader of the block data by its `Cid`
    pub fn get_reader(&self, k: Cid) -> anyhow::Result<Option<impl Read>> {
        for position in self.indexed.get(k)? {
            // escape the positioned_io::Slice
            let entire_file = self.indexed.reader().get_ref();
            // `position` is the frame start offset.
            let cursor = Cursor::new_pos(entire_file, position);
            let mut decoder = zstd::Decoder::new(cursor)?.single_frame();
            while let Ok(car_block_len) = decoder.read_varint::<usize>() {
                let cid = Cid::read_bytes(&mut decoder)?;
                let data_len = car_block_len.saturating_sub(cid.encoded_len()) as u64;
                if cid == k {
                    // return the reader instead of decoding the entire data block into memory
                    return Ok(Some(decoder.take(data_len)));
                }
                // Discard data bytes
                io::copy(&mut decoder.by_ref().take(data_len), &mut io::sink())?;
            }
        }
        Ok(None)
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
        let indexed = &self.indexed;
        for position in indexed.get(*k)?.into_iter() {
            let cache_query = self.frame_cache.get(position, self.cache_key, *k);
            match cache_query {
                // Frame cache hit, found value.
                Some(Some(val)) => return Ok(Some(val)),
                // Frame cache hit, no value. This only happens when hashes collide
                Some(None) => {}
                None => {
                    // Decode entire frame into memory, "position" arg is the frame start offset.
                    let entire_file = indexed.reader().get_ref(); // escape the positioned_io::Slice
                    let cursor = Cursor::new_pos(entire_file, position);
                    let mut zstd_frame = decode_zstd_single_frame(cursor)?;
                    // Parse all key-value pairs and insert them into a map
                    let mut block_map = hashbrown::HashMap::new();
                    while let Some(block_frame) = uvi_bytes().decode_eof(&mut zstd_frame)? {
                        let CarBlock { cid, data } = CarBlock::from_bytes(block_frame)?;
                        block_map.insert(cid.into(), data);
                    }
                    let get_result = block_map.get(&CidWrapper::from(*k)).cloned();
                    self.frame_cache.put(position, self.cache_key, block_map);

                    // This lookup only fails in case of a hash collision
                    if let Some(value) = get_result {
                        return Ok(Some(value));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Not supported, use [`super::ManyCar`] instead.
    fn put_keyed(&self, _: &Cid, _: &[u8]) -> anyhow::Result<()> {
        unreachable!("ForestCar is read-only, use ManyCar instead");
    }
}

fn decode_zstd_single_frame<ReaderT: Read>(reader: ReaderT) -> io::Result<BytesMut> {
    let mut zstd_frame = vec![];
    zstd::Decoder::new(reader)?
        .single_frame()
        .read_to_end(&mut zstd_frame)?;
    Ok(zstd_frame.into_iter().collect())
}

pub struct Encoder {}

impl Encoder {
    pub async fn write(
        mut sink: impl AsyncWrite + Unpin,
        roots: NonEmpty<Cid>,
        mut stream: impl Stream<Item = anyhow::Result<ForestCarFrame>> + Unpin,
    ) -> anyhow::Result<()> {
        let mut offset = 0;

        // Write CARv1 header
        let mut header_encoder = new_encoder(DEFAULT_FOREST_CAR_COMPRESSION_LEVEL)?;

        let header = CarV1Header { roots, version: 1 };
        let mut header_uvi_frame = BytesMut::new();
        uvi_bytes().encode(
            Bytes::from(fvm_ipld_encoding::to_vec(&header)?),
            &mut header_uvi_frame,
        )?;
        header_encoder.write_all(&header_uvi_frame)?;
        let header_bytes = header_encoder.finish()?.into_inner().freeze();

        sink.write_all(&header_bytes).await?;
        let header_len = header_bytes.len();

        offset += header_len;

        // Write seekable zstd and collect a mapping of CIDs to frame_offset+data_offset.
        let mut builder = index::Builder::new();
        while let Some((cids, zstd_frame)) = stream.try_next().await? {
            builder.extend(cids.into_iter().map(|cid| (cid, offset as u64)));
            sink.write_all(&zstd_frame).await?;
            offset += zstd_frame.len()
        }

        // Create index
        let writer = builder.into_writer();
        write_skip_frame_header_async(&mut sink, writer.written_len().try_into().unwrap()).await?;
        writer.write_into(&mut sink).await?;

        // Write ForestCAR.zst footer, it's a valid ZSTD skip-frame
        let footer = ForestCarFooter {
            index: offset as u64 + ZSTD_SKIP_FRAME_LEN,
        };
        sink.write_all(&footer.to_le_bytes()).await?;
        Ok(())
    }

    /// `compress_stream` with [`DEFAULT_FOREST_CAR_FRAME_SIZE`] as default frame size and [`DEFAULT_FOREST_CAR_COMPRESSION_LEVEL`] as default compression level.
    pub fn compress_stream_default(
        stream: impl Stream<Item = anyhow::Result<CarBlock>>,
    ) -> impl Stream<Item = anyhow::Result<ForestCarFrame>> {
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
        stream: impl Stream<Item = anyhow::Result<CarBlock>>,
    ) -> impl Stream<Item = anyhow::Result<ForestCarFrame>> {
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
                    Some(Err(e)) => {
                        return Poll::Ready(Some(Err(anyhow::anyhow!(
                            "error polling CarBlock from stream: {e}"
                        ))));
                    }
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

pub fn finalize_frame(
    zstd_compression_level: u16,
    encoder: &mut zstd::Encoder<'static, Writer<BytesMut>>,
) -> io::Result<Bytes> {
    let prev_encoder = std::mem::replace(encoder, new_encoder(zstd_compression_level)?);
    Ok(prev_encoder.finish()?.into_inner().freeze())
}

pub fn new_encoder(
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
        let mut buffer = [0; 16];
        // Skippable frames start with 50 2A 4D 18
        buffer[0..4].copy_from_slice(&ZSTD_SKIPPABLE_FRAME_MAGIC_HEADER);
        // Then a u32 containing the length of the data in the frame
        buffer[4..8].copy_from_slice(&(std::mem::size_of_val(&self.index) as u32).to_le_bytes());
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

pub fn new_forest_car_temp_path_in(
    output_dir: impl AsRef<Path>,
) -> std::io::Result<tempfile::TempPath> {
    Ok(tempfile::Builder::new()
        .suffix(TEMP_FOREST_CAR_FILE_EXTENSION)
        .tempfile_in(output_dir)?
        .into_temp_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_on;
    use nunny::vec as nonempty;
    use quickcheck_macros::quickcheck;

    fn mk_encoded_car(
        zstd_frame_size_tripwire: usize,
        zstd_compression_level: u16,
        roots: NonEmpty<Cid>,
        blocks: NonEmpty<CarBlock>,
    ) -> Vec<u8> {
        block_on(async {
            let frame_stream = Encoder::compress_stream(
                zstd_frame_size_tripwire,
                zstd_compression_level,
                futures::stream::iter(blocks.into_iter().map(Ok)),
            );
            let mut encoded = vec![];
            Encoder::write(&mut encoded, roots, frame_stream)
                .await
                .unwrap();
            encoded
        })
    }

    #[quickcheck]
    fn forest_car_create_basic(blocks: nunny::Vec<CarBlock>) {
        let roots = nonempty!(blocks.first().cid);
        let forest_car =
            ForestCar::new(mk_encoded_car(1024 * 4, 3, roots.clone(), blocks.clone())).unwrap();
        assert_eq!(forest_car.head_tipset_key(), &roots);
        for block in blocks {
            assert_eq!(forest_car.get(&block.cid).unwrap().unwrap(), block.data);
            let mut buf = vec![];
            forest_car
                .get_reader(block.cid)
                .unwrap()
                .unwrap()
                .read_to_end(&mut buf)
                .unwrap();
            assert_eq!(buf, block.data);
        }
    }

    #[quickcheck]
    fn forest_car_create_options(
        blocks: nunny::Vec<CarBlock>,
        frame_size: usize,
        mut compression_level: u16,
    ) {
        compression_level %= 15;
        let roots = nonempty!(blocks.first().cid);

        let forest_car = ForestCar::new(mk_encoded_car(
            frame_size,
            compression_level.max(1),
            roots.clone(),
            blocks.clone(),
        ))
        .unwrap();
        assert_eq!(forest_car.head_tipset_key(), &roots);
        for block in blocks {
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
        use crate::utils::multihash::prelude::*;

        // Distinct CIDs may map to the same hash value
        let cid_a = Cid::new_v1(0, MultihashCode::Identity.digest(&[10]));
        let cid_b = Cid::new_v1(0, MultihashCode::Identity.digest(&[0]));
        // A and B are _not_ the same...
        assert_ne!(cid_a, cid_b);
        // ... but they map to the same hash:
        assert_eq!(index::hash::summary(&cid_a), index::hash::summary(&cid_b));

        // For testing purposes, we ignore that the data doesn't map to the
        // CIDs.
        let blocks = nonempty![
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
        let forest_car = ForestCar::new(mk_encoded_car(
            0,
            3,
            nonempty![blocks.first().cid],
            blocks.clone(),
        ))
        .unwrap();

        // Even with colliding hashes, the CIDs can still be queried:
        assert_eq!(forest_car.get(&cid_a).unwrap().unwrap(), blocks[0].data);
        assert_eq!(forest_car.get(&cid_b).unwrap().unwrap(), blocks[1].data);
    }
}
