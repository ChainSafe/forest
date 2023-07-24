// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(dead_code)]

//! # Varint frames
//!
//! CARs are made of concatenations of _varint frames_. Each varint frame is a
//! concatenation of the _body length_ as an
//! [varint](https://docs.rs/integer-encoding/4.0.0/integer_encoding/trait.VarInt.html),
//! and the _frame body_ itself. [`crate::utils::encoding::UviBytes`] can be
//! used to read frames piecewise into memory.
//!
//! ```text
//!        varint frame
//! │◄───────────────────────►│
//! │                         │
//! ├───────────┬─────────────┤
//! │varint:    │             │
//! │body length│frame body   │
//! └───────────┼─────────────┤
//!             │             │
//! frame body ►│◄───────────►│
//!     offset     =body length
//! ```
//!
//! # CARv1 layout and seeking
//!
//! The first varint frame is a _header frame_, where the frame body is a [`CarHeader`] encoded using [`ipld_dagcbor`](serde_ipld_dagcbor).
//!
//! Subsequent varint frames are _block frames_, where the frame body is a concatenation of a [`Cid`] and the _block data_ addressed by that CID.
//!
//! ```text
//! block frame ►│
//! body offset  │
//!              │  =body length
//!              │◄────────────►│
//!  ┌───────────┼───┬──────────┤
//!  │body length│cid│block data│
//!  └───────────┴───┼──────────┤
//!                  │◄────────►│
//!                  │  =block data length
//!      block data  │
//!          offset ►│
//! ```
//!
//! ## Block ordering
//! > _... a filecoin-deterministic car-file is currently implementation-defined as containing all DAG-forming blocks in first-seen order, as a result of a depth-first DAG traversal starting from a single root._
//! - [CAR documentation](https://ipld.io/specs/transport/car/carv1/#determinism)
//!
//! # Future work
//! - [`fadvise`](https://linux.die.net/man/2/posix_fadvise)-based APIs to pre-fetch parts of the file, to improve random access performance.
//! - Use an inner [`Blockstore`] for writes.
//! - Use safe arithmetic for all operations - a malicious frame shouldn't cause a crash.
//! - Theoretically, file-backed blockstores should be clonable (or even [`Sync`]) with very low overhead, so that multiple threads could perform operations concurrently.
//! - CARv2 support
//! - A wrapper that abstracts over car formats for reading.

use crate::utils::db::car_stream::Block;
use ahash::HashMapExt as _;
use bytes::{buf::Writer, BufMut as _, Bytes, BytesMut};
use cid::Cid;
use futures::{stream, Stream, StreamExt as _, TryStream, TryStreamExt as _};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_car::CarHeader;
use integer_encoding::{VarInt, VarIntReader};
use itertools::Itertools as _;
use parking_lot::Mutex;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{
    any::Any,
    collections::hash_map::Entry::{Occupied, Vacant},
    future,
    io::{
        self, BufRead, BufReader,
        ErrorKind::{InvalidData, Other, UnexpectedEof, Unsupported},
        Read, Seek, SeekFrom, Write as _,
    },
    iter,
    ops::ControlFlow,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{BytesCodec, FramedRead, FramedWrite};
use tracing::{debug, trace};

use crate::utils::{try_collate, Collate};

/// **Note that all operations on this store are blocking**.
///
/// It can often be time, memory, or disk prohibitive to read large snapshots into a database like [`ParityDb`](crate::db::parity_db::ParityDb).
///
/// This is an implementer of [`Blockstore`] that simply wraps an uncompressed [CARv1 file](https://ipld.io/specs/transport/car/carv1).
///
/// On creation, [`UncompressedCarV1BackedBlockstore`] builds an in-memory index of the [`Cid`]s in the file,
/// and their offsets into that file.
/// Note that it prepares its own buffer for doing so.
///
/// When a block is requested, [`UncompressedCarV1BackedBlockstore`] scrolls to that offset, and reads the block, on-demand.
///
/// Writes for new blocks (which don't exist in the CAR already) are currently cached in-memory.
///
/// Random-access performance is expected to be poor, as the OS will have to load separate parts of the file from disk, and flush it for each read.
/// However, (near) linear access should be pretty good, as file chunks will be pre-fetched.
///
/// See [module documentation](mod@self) for more.
pub struct UncompressedCarV1BackedBlockstore<ReaderT> {
    // https://github.com/ChainSafe/forest/issues/3096
    inner: Mutex<UncompressedCarV1BackedBlockstoreInner<ReaderT>>,
}

impl<ReaderT> UncompressedCarV1BackedBlockstore<ReaderT>
where
    ReaderT: Read + Seek,
{
    /// To be correct:
    /// - `reader` must read immutable data. e.g if it is a file, it should be [`flock`](https://linux.die.net/man/2/flock)ed.
    ///   [`Blockstore`] API calls may panic if this is not upheld.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn new(mut reader: ReaderT) -> io::Result<Self> {
        let roots = get_roots_from_v1_header(&mut reader)?;

        // When indexing, we perform small reads of the length and CID before seeking
        // Buffering these gives us a ~50% speedup (n=10): https://github.com/ChainSafe/forest/pull/3085#discussion_r1246897333
        let mut buf_reader = BufReader::with_capacity(1024, reader);

        // now create the index
        let index =
            iter::from_fn(|| read_block_data_location_and_skip(&mut buf_reader).transpose())
                .collect::<Result<ahash::HashMap<_, _>, _>>()?;

        match index.len() {
            0 => Err(io::Error::new(
                InvalidData,
                "CARv1 files must contain at least one block",
            )),
            num_blocks => {
                debug!(num_blocks, "indexed CAR");
                Ok(Self {
                    inner: Mutex::new(UncompressedCarV1BackedBlockstoreInner {
                        // discarding the buffer is ok - we only seek within this now
                        reader: buf_reader.into_inner(),
                        index,
                        roots,
                        write_cache: ahash::HashMap::new(),
                    }),
                })
            }
        }
    }

    pub fn roots(&self) -> Vec<Cid> {
        self.inner.lock().roots.clone()
    }

    /// In an arbitrary order
    #[cfg(test)]
    pub fn cids(&self) -> Vec<Cid> {
        self.inner.lock().index.keys().cloned().collect()
    }
}

struct UncompressedCarV1BackedBlockstoreInner<ReaderT> {
    reader: ReaderT,
    write_cache: ahash::HashMap<Cid, Vec<u8>>,
    index: ahash::HashMap<Cid, UncompressedBlockDataLocation>,
    roots: Vec<Cid>,
}

/// If you seek to `offset` (from the start of the file), and read `length` bytes,
/// you should get data that corresponds to a [`Cid`] (but NOT the [`Cid`] itself).
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct UncompressedBlockDataLocation {
    offset: u64,
    length: u32,
}

impl<ReaderT> Blockstore for UncompressedCarV1BackedBlockstore<ReaderT>
where
    ReaderT: Read + Seek,
{
    #[tracing::instrument(level = "trace", skip(self))]
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        let UncompressedCarV1BackedBlockstoreInner {
            reader,
            write_cache,
            index,
            ..
        } = &mut *self.inner.lock();
        match (index.get(k), write_cache.entry(*k)) {
            (Some(_location), Occupied(cached)) => {
                trace!("evicting from write cache");
                Ok(Some(cached.remove()))
            }
            (Some(UncompressedBlockDataLocation { offset, length }), Vacant(_)) => {
                trace!("fetching from disk");
                reader.seek(SeekFrom::Start(*offset))?;
                let mut data = vec![0; usize::try_from(*length).unwrap()];
                reader.read_exact(&mut data)?;
                Ok(Some(data))
            }
            (None, Occupied(cached)) => {
                trace!("getting from write cache");
                Ok(Some(cached.get().clone()))
            }
            (None, Vacant(_)) => {
                trace!("not found");
                Ok(None)
            }
        }
    }

    /// # Panics
    /// - If the write cache already contains different data with this CID
    /// - See also [`Self::new`].
    #[tracing::instrument(level = "trace", skip(self, block))]
    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        let UncompressedCarV1BackedBlockstoreInner {
            write_cache, index, ..
        } = &mut *self.inner.lock();
        handle_write_cache(write_cache, index, k, block)
    }
}

/// **Note that all operations on this store are blocking**.
///
/// Similar to [`UncompressedCarV1BackedBlockstore`], this blockstore wraps a CAR file on-disk,
/// but notably that file is [zstd compressed](http://facebook.github.io/zstd/).
///
/// Seeking through a compressed file is non-trivial, as to uncompress a byte at N, you must first
/// decompress ALL preceding bytes, which precludes trivial random access.
///
/// However, the zstd format is frame oriented - each successive frame may be uncompressed independently.
///
/// It can still be practical to randomly seek through a zstd compressed file, if the zstd frames are small.
///
/// This blockstore also requires the zstd frames to align with the varint frames:
/// ```text
/// ┌────────────────────┐
/// │zstd frame          │
/// ├──────┬──────┬──────┤
/// │varint│varint│varint│
/// │frame │frame │frame │
/// └──────┴──────┴──────┘
/// ```
/// [`zstd_compress_varint_manyframe`] can be used to prepare such a file.
/// This makes our code much simpler. However, once rust support for the
/// [zstd seekable extension format](https://github.com/facebook/zstd/blob/118200f7b95deaf38b3368cb445a564f187da1a2/contrib/seekable_format/zstd_seekable_compression_format.md)
/// is better, this restriction could be lifted.
pub struct CompressedCarV1BackedBlockstore<ReaderT> {
    // https://github.com/ChainSafe/forest/issues/3096
    inner: Mutex<CompressedCarV1BackedBlockstoreInner<ReaderT>>,
}

impl<ReaderT> CompressedCarV1BackedBlockstore<ReaderT> {
    /// returns an [`Other`] error containing a [`MaxFrameSizeExceeded`] when the `reader`'s file
    /// has frames which are too large.
    /// See the documentation for [`Self`] for more.
    ///
    /// To be correct:
    /// - `reader` must read immutable data. e.g if it is a file, it should be [`flock`](https://linux.die.net/man/2/flock)ed.
    ///   [`Blockstore`] API calls may panic if this is not upheld.
    // This used to avoid reading entire zstd frames in, but we're going to read-cache, so may as well
    // rewrite the whole thing to uncompress a frame at a time.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn new(mut reader: ReaderT) -> io::Result<Self>
    where
        ReaderT: BufRead + Seek,
    {
        let mut zstd_frames = ZstdFrames::new(&mut reader, 1_000_000_u64.next_power_of_two());
        let (first_zstd_frame_offset, mut first_zstd_frame) = zstd_frames
            .next()
            .ok_or(io::Error::new(InvalidData, "CAR must not be empty"))??;

        let roots = get_roots_from_v1_header(&mut first_zstd_frame)?;
        let mut index =
            iter::from_fn(|| read_block_data_location_and_skip(&mut first_zstd_frame).transpose())
                .map_ok(|(cid, location_in_frame)| {
                    (
                        cid,
                        CompressedBlockDataLocation {
                            zstd_frame_offset: first_zstd_frame_offset,
                            location_in_frame,
                        },
                    )
                })
                .collect::<Result<ahash::HashMap<_, _>, _>>()?;

        for maybe_frame in zstd_frames {
            let (zstd_frame_offset, mut zstd_frame) = maybe_frame?;
            index.extend(
                iter::from_fn(|| read_block_data_location_and_skip(&mut zstd_frame).transpose())
                    .map_ok(|(cid, location_in_frame)| {
                        (
                            cid,
                            CompressedBlockDataLocation {
                                zstd_frame_offset,
                                location_in_frame,
                            },
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )
        }

        match index.len() {
            0 => Err(io::Error::new(
                InvalidData,
                "CARv1 files must contain at least one block",
            )),
            num_blocks => {
                debug!(num_blocks, "indexed CAR");
                Ok(Self {
                    inner: Mutex::new(CompressedCarV1BackedBlockstoreInner {
                        reader,
                        write_cache: ahash::HashMap::new(),
                        most_recent_zstd_frame: None,
                        index,
                        roots,
                    }),
                })
            }
        }
    }

    #[cfg(test)]
    /// `index` must correspond to the `reader`. [`Blockstore`] API calls may panic if this is not upheld
    ///
    ///  See also [`Self::new`]
    // TODO(aatifsyed): do we want to check that `reader` contains e.g the `roots`? That `index` is non-empty?
    pub fn new_with_trusted_index(
        reader: ReaderT,
        index: ahash::HashMap<Cid, CompressedBlockDataLocation>,
        roots: Vec<Cid>,
    ) -> Self {
        Self {
            inner: Mutex::new(CompressedCarV1BackedBlockstoreInner {
                reader,
                write_cache: ahash::HashMap::new(),
                most_recent_zstd_frame: None,
                index,
                roots,
            }),
        }
    }

    pub fn roots(&self) -> Vec<Cid> {
        self.inner.lock().roots.clone()
    }

    /// In an arbitrary order
    #[cfg(test)]
    pub fn cids(&self) -> Vec<Cid> {
        self.inner.lock().index.keys().cloned().collect()
    }
}

use crate::utils::db::car_index;

pub fn keys_from_compressed_car<ReaderT>(
    mut reader: ReaderT,
) -> io::Result<Vec<(Cid, car_index::BlockPosition)>>
where
    ReaderT: BufRead + Seek,
{
    let mut zstd_frames = ZstdFrames::new(&mut reader, 1_000_000_u64.next_power_of_two());
    let (first_zstd_frame_offset, mut first_zstd_frame) = zstd_frames
        .next()
        .ok_or(io::Error::new(InvalidData, "CAR must not be empty"))??;

    let mut index =
        iter::from_fn(|| read_block_frame_location_and_skip(&mut first_zstd_frame).transpose())
            .map_ok(|(cid, location_in_frame)| {
                (
                    cid,
                    car_index::BlockPosition::new(
                        first_zstd_frame_offset,
                        u16::try_from(location_in_frame)
                            .expect(&format!("offset too large {location_in_frame}")),
                    )
                    .unwrap(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

    for maybe_frame in zstd_frames {
        let (zstd_frame_offset, mut zstd_frame) = maybe_frame?;
        index.extend(
            iter::from_fn(|| read_block_frame_location_and_skip(&mut zstd_frame).transpose())
                .map_ok(|(cid, location_in_frame)| {
                    (
                        cid,
                        car_index::BlockPosition::new(
                            zstd_frame_offset,
                            u16::try_from(location_in_frame)
                                .expect(&format!("offset too large {location_in_frame}")),
                        )
                        .unwrap(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
    }
    match index.len() {
        0 => Err(io::Error::new(
            InvalidData,
            "CARv1 files must contain at least one block",
        )),
        num_blocks => {
            debug!(num_blocks, "indexed CAR");
            Ok(index)
        }
    }
}

pub fn write_skip_frame(mut writer: impl std::io::Write, frame: &[u8]) -> std::io::Result<()> {
    // writer.write_all(&[0x18,0x4D,0x2A,0x50])?;
    writer.write_all(&[0x50, 0x2A, 0x4D, 0x18])?;
    let len: u32 = frame.len() as u32;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(frame)
}

struct CompressedCarV1BackedBlockstoreInner<ReaderT> {
    reader: ReaderT,
    write_cache: ahash::HashMap<Cid, Vec<u8>>,
    // zstd frame offset, zstd frame contents
    most_recent_zstd_frame: Option<(u64, std::io::Cursor<Vec<u8>>)>,
    index: ahash::HashMap<Cid, CompressedBlockDataLocation>,
    roots: Vec<Cid>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CompressedBlockDataLocation {
    pub zstd_frame_offset: u64,
    pub location_in_frame: UncompressedBlockDataLocation,
}

impl<ReaderT> Blockstore for CompressedCarV1BackedBlockstore<ReaderT>
where
    ReaderT: Read + Seek,
{
    #[tracing::instrument(level = "trace", skip(self))]
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        let CompressedCarV1BackedBlockstoreInner {
            reader,
            write_cache,
            index,
            most_recent_zstd_frame,
            ..
        } = &mut *self.inner.lock();
        match (index.get(k), write_cache.entry(*k)) {
            (Some(_location), Occupied(cached)) => {
                trace!("evicting from write cache");
                Ok(Some(cached.remove()))
            }
            (
                Some(CompressedBlockDataLocation {
                    zstd_frame_offset,
                    location_in_frame: UncompressedBlockDataLocation { offset, length },
                }),
                Vacant(_),
            ) => {
                let zstd_frame = match most_recent_zstd_frame.as_mut() {
                    Some((offset, most_recent_zstd_frame)) if offset == zstd_frame_offset => {
                        trace!("read cache hit");
                        most_recent_zstd_frame
                    }
                    Some(_) | None => {
                        trace!("read cache miss, reading from disk");
                        reader.seek(SeekFrom::Start(*zstd_frame_offset))?;
                        let mut zstd_frame = std::io::Cursor::new(vec![]);
                        zstd::Decoder::new(reader)
                            .expect("we're not using a custom dictionary")
                            .single_frame()
                            .read_to_end(zstd_frame.get_mut())?;
                        let (_, inserted_zstd_frame) =
                            most_recent_zstd_frame.insert((*zstd_frame_offset, zstd_frame));
                        inserted_zstd_frame
                    }
                };
                zstd_frame
                    .seek(SeekFrom::Start(*offset))
                    .expect("index offset is incorrect");
                let mut data = vec![0; usize::try_from(*length).unwrap()];
                zstd_frame.read_exact(&mut data)?;
                Ok(Some(data))
            }
            (None, Occupied(cached)) => {
                trace!("getting from write cache");
                Ok(Some(cached.get().clone()))
            }
            (None, Vacant(_)) => {
                trace!("not found");
                Ok(None)
            }
        }
    }

    /// # Panics
    /// - If the write cache contains different data with this CID
    /// - See also [`Self::new`].
    #[tracing::instrument(level = "trace", skip(self, block))]
    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        let CompressedCarV1BackedBlockstoreInner {
            write_cache, index, ..
        } = &mut *self.inner.lock();
        handle_write_cache(write_cache, index, k, block)
    }
}

/// # Panics
/// - If the write cache already contains different data with this CID
fn handle_write_cache(
    write_cache: &mut ahash::HashMap<Cid, Vec<u8>>,
    index: &mut ahash::HashMap<Cid, impl Any>,
    k: &Cid,
    block: &[u8],
) -> anyhow::Result<()> {
    match (index.get(k), write_cache.entry(*k)) {
        (None, Occupied(already)) => match already.get() == block {
            true => {
                trace!("already in cache");
                Ok(())
            }
            false => panic!("mismatched content on second write for CID {k}"),
        },
        (None, Vacant(vacant)) => {
            trace!(bytes = block.len(), "insert into cache");
            vacant.insert(block.to_owned());
            Ok(())
        }
        (Some(_), Vacant(_)) => {
            trace!("already on disk");
            Ok(())
        }
        (Some(_), Occupied(_)) => {
            unreachable!("we don't insert a CID in the write cache if it exists on disk")
        }
    }
}

fn get_roots_from_v1_header(reader: impl Read) -> io::Result<Vec<Cid>> {
    match read_header(reader)? {
        CarHeader { roots, version: 1 } if !roots.is_empty() => Ok(roots),
        _other_version => Err(io::Error::new(
            Unsupported,
            "header must be CARv1 with non-empty roots",
        )),
    }
}

fn cid_error_to_io_error(cid_error: cid::Error) -> io::Error {
    match cid_error {
        cid::Error::Io(io_error) => io_error,
        other => io::Error::new(InvalidData, other),
    }
}

/// ```text
/// start ►│          reader end ►│
///        ├───────────┬──────────┤
///        │body length│car header│
///        └───────────┴──────────┘
/// ```
#[tracing::instrument(level = "trace", skip_all, ret, err)]
fn read_header(mut reader: impl Read) -> io::Result<CarHeader> {
    let header_len =
        read_varint_body_length_or_eof(&mut reader)?.ok_or(io::Error::from(UnexpectedEof))?;
    let mut buffer = vec![0; usize::try_from(header_len).unwrap()];
    reader.read_exact(&mut buffer)?;
    fvm_ipld_encoding::from_slice(&buffer).map_err(|e| io::Error::new(InvalidData, e))
}

/// Returns ([`Cid`], the `block data offset` and `block data length`)
/// ```text
/// start ►│              reader end ►│
///        ├───────────┬───┬──────────┤
///        │body length│cid│block data│
///        └───────────┴───┼──────────┤
///                        │◄────────►│
///                        │  =block data length
///            block data  │
///                offset ►│
/// ```
/// Importantly, we seek `block data length`, rather than read any in.
/// This allows us to keep indexing fast.
///
/// [`Ok(None)`] on EOF
#[tracing::instrument(level = "trace", skip_all, ret)]
fn read_block_data_location_and_skip(
    mut reader: (impl Read + Seek),
) -> io::Result<Option<(Cid, UncompressedBlockDataLocation)>> {
    let Some(body_length) = read_varint_body_length_or_eof(&mut reader)? else {
        return Ok(None);
    };
    let frame_body_offset = reader.stream_position()?;
    let mut reader = CountRead::new(&mut reader);
    let cid = Cid::read_bytes(&mut reader).map_err(cid_error_to_io_error)?;

    // counting the read bytes saves us a syscall for finding block data offset
    let cid_length = reader.bytes_read();
    let block_data_offset = frame_body_offset + u64::try_from(cid_length).unwrap();
    let next_frame_offset = frame_body_offset + u64::from(body_length);
    let block_data_length = u32::try_from(next_frame_offset - block_data_offset).unwrap();
    reader
        .into_inner()
        .seek(SeekFrom::Start(next_frame_offset))?;
    Ok(Some((
        cid,
        UncompressedBlockDataLocation {
            offset: block_data_offset,
            length: block_data_length,
        },
    )))
}

fn read_block_frame_location_and_skip(
    mut reader: (impl Read + Seek),
) -> io::Result<Option<(Cid, u64)>> {
    let Some(body_length) = read_varint_body_length_or_eof(&mut reader)? else {
        return Ok(None);
    };
    let frame_body_offset = reader.stream_position()?;
    // eprintln!("Reading entry at: {frame_body_offset}");
    let mut reader = CountRead::new(&mut reader);
    let cid = Cid::read_bytes(&mut reader).map_err(cid_error_to_io_error)?;

    // counting the read bytes saves us a syscall for finding block data offset
    // let cid_length = reader.bytes_read();
    let next_frame_offset = frame_body_offset + u64::from(body_length);
    reader
        .into_inner()
        .seek(SeekFrom::Start(next_frame_offset))?;
    Ok(Some((cid, frame_body_offset)))
}

/// Reads `body length`, leaving the reader at the start of a varint frame,
/// or returns [`Ok(None)`] if we've reached EOF
/// ```text
/// start ►│
///        ├───────────┬─────────────┐
///        │varint:    │             │
///        │body length│frame body   │
///        └───────────┼─────────────┘
///        reader end ►│
/// ```
fn read_varint_body_length_or_eof(mut reader: impl Read) -> io::Result<Option<u32>> {
    let mut byte = [0u8; 1]; // detect EOF
    match reader.read(&mut byte)? {
        0 => Ok(None),
        1 => (byte.chain(reader)).read_varint().map(Some),
        _ => unreachable!(),
    }
}

/// A reader that keeps track of how many bytes it has read.
///
/// This is useful for calculating the _block data length_ when the (_varint frame_) _body length_ is known.
struct CountRead<ReadT> {
    inner: ReadT,
    count: usize,
}

impl<ReadT> CountRead<ReadT> {
    pub fn new(inner: ReadT) -> Self {
        Self { inner, count: 0 }
    }
    pub fn bytes_read(&self) -> usize {
        self.count
    }
    pub fn into_inner(self) -> ReadT {
        self.inner
    }
}

impl<ReadT> Read for CountRead<ReadT>
where
    ReadT: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count += n;
        Ok(n)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("zstd frame exceeds configured max frame size")]
pub struct MaxFrameSizeExceeded;

/// An iterator over offsets and contents of zstd frames.
///
/// Note that each iteration reads an entire frame into memory, and typical zstd compressed files
/// are single-frame.
///
/// As such, there is a configurable `max_frame_size`, which causes the iterator to return a [`Other`] error containing a [`MaxFrameSizeExceeded`] when hit.
///
/// After such an error, the iterator should be considered unrecoverable, and discarded.
pub struct ZstdFrames<ReaderT> {
    inner: ReaderT,
    max_frame_size: u64,
}

impl<ReaderT> ZstdFrames<ReaderT> {
    pub fn new(inner: ReaderT, max_frame_size: u64) -> Self {
        Self {
            inner,
            max_frame_size,
        }
    }
}

impl<ReaderT> Iterator for ZstdFrames<ReaderT>
where
    ReaderT: BufRead + Seek,
{
    type Item = io::Result<(u64, std::io::Cursor<Vec<u8>>)>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut v = vec![];
        match self.inner.stream_position().and_then(|offset| {
            // we MUST have a BufReader here - otherwise zstd::Decoder creates an internal buffer, and its contents is lost on the next iteration
            let decoder = zstd::Decoder::with_buffer((&mut self.inner).take(self.max_frame_size))
                .expect("we're not using a custom dictionary");
            decoder
                .single_frame()
                .read_to_end(&mut v)
                .map(|_num_bytes| offset)
        }) {
            Ok(offset) => {
                // let new_pos = self.inner.stream_position().unwrap();
                // eprintln!("Uncompressed: {}, compressed: {}", v.len(), new_pos-offset);
                Some(Ok((offset, std::io::Cursor::new(v))))
            }
            Err(e) if e.kind() == UnexpectedEof && v.is_empty() => None,
            Err(e)
                if e.kind() == UnexpectedEof
                    && u64::try_from(v.len()).unwrap() >= self.max_frame_size =>
            {
                Some(Err(io::Error::new(Other, MaxFrameSizeExceeded)))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

pin_project! {
    struct ZstdCompressBlock<Inner> {
        #[pin]
        inner: Inner,
        zstd_frame_size_tripwire: usize,
        zstd_compression_level: u16,
        encoder: zstd::Encoder<'static, Writer<BytesMut>>,
    }
}

impl<Inner: TryStream<Ok = Block, Error = io::Error>> ZstdCompressBlock<Inner> {
    fn new(
        inner: Inner,
        zstd_frame_size_tripwire: usize,
        zstd_compression_level: u16,
    ) -> io::Result<Self> {
        let encoder =
            zstd::Encoder::new(BytesMut::new().writer(), i32::from(zstd_compression_level))?;
        Ok(ZstdCompressBlock {
            inner,
            zstd_frame_size_tripwire,
            zstd_compression_level,
            encoder,
        })
    }
}

impl<Inner: TryStream<Ok = Block, Error = io::Error>> Stream for ZstdCompressBlock<Inner> {
    type Item = io::Result<(Bytes, ahash::HashMap<Cid, u64>)>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        if let Some(item) = futures::ready!(this.inner.try_poll_next(cx)) {
            match item {
                Err(e) => Poll::Ready(Some(Err(e))),
                Ok(block) => {
                    block.write(this.encoder).unwrap();
                    this.encoder.flush().unwrap();
                    let compressed_len = this.encoder.get_ref().get_ref().len();
                    if compressed_len >= *this.zstd_frame_size_tripwire {
                        let new_encoder = zstd::Encoder::new(
                            BytesMut::new().writer(),
                            i32::from(*this.zstd_compression_level),
                        )
                        .unwrap();
                        let encoder = std::mem::replace(this.encoder, new_encoder);
                        let frame = encoder
                            .finish()
                            .expect("BytesMut has infallible IO")
                            .into_inner()
                            .freeze();
                        Poll::Ready(Some(Ok((frame, ahash::HashMap::default()))))
                    } else {
                        Poll::Pending
                    }
                }
            }
        } else {
            Poll::Ready(None)
        }
    }
}

fn append_block_to_zstd_encoder(
    encoder: &mut zstd::Encoder<'static, Writer<BytesMut>>,
    block: Block,
    zstd_frame_size_tripwire: usize,
    zstd_compression_level: u16,
) -> io::Result<Option<Bytes>> {
    block.write(encoder)?;
    encoder.flush()?;
    let compressed_len = encoder.get_ref().get_ref().len();
    if compressed_len >= zstd_frame_size_tripwire {
        let new_encoder =
            zstd::Encoder::new(BytesMut::new().writer(), i32::from(zstd_compression_level))
                .unwrap();
        let encoder = std::mem::replace(encoder, new_encoder);
        let frame = encoder
            .finish()
            .expect("BytesMut has infallible IO")
            .into_inner()
            .freeze();
        Ok(Some(frame))
    } else {
        Ok(None)
    }
}

pub async fn zstd_compress_blocks(
    reader: impl TryStream<Ok = Block, Error = io::Error>,
    zstd_frame_size_tripwire: usize,
    zstd_compression_level: u16,
) -> io::Result<impl Stream<Item = io::Result<(Bytes, ahash::HashMap<Cid, u64>)>>> {
    ZstdCompressBlock::new(reader, zstd_frame_size_tripwire, zstd_compression_level)
}

type VarintFrameCodec = unsigned_varint::codec::UviBytes<BytesMut>;

/// `reader` reads uncompressed [varint frames](index.html#varint-frames).
///
/// This function repeatedly takes a group of successive varint frames from `reader`,
/// and compresses them into one zstd frame, which is written to `writer`.
///
/// Each group will be bigger than `zstd_frame_size_tripwire` by at most one frame (compressed).
///
/// returns the number of frames written.
pub async fn zstd_compress_varint_manyframe(
    reader: impl AsyncRead,
    writer: impl AsyncWrite,
    zstd_frame_size_tripwire: usize,
    zstd_compression_level: u16,
) -> io::Result<usize> {
    type VarintFrameCodec = crate::utils::encoding::UviBytes;
    let mut count = 0;
    try_collate(
        FramedRead::new(reader, VarintFrameCodec::default()),
        varint_to_zstd_frame_collator(zstd_frame_size_tripwire, zstd_compression_level),
        zstd_compress_finish,
    )
    .inspect_ok(|_| count += 1)
    .forward(FramedWrite::new(writer, BytesCodec::new()))
    .await?;
    Ok(count)
}

/// Create a parameterized collator function
fn varint_to_zstd_frame_collator(
    zstd_frame_size_tripwire: usize,
    zstd_compression_level: u16,
) -> impl Fn(
    Collate<zstd::Encoder<'_, Writer<BytesMut>>, BytesMut>,
) -> ControlFlow<BytesMut, zstd::Encoder<'_, Writer<BytesMut>>> {
    move |collate| {
        let mut encoder = match collate {
            Collate::Started(body) => zstd_compress_fold_varint_frame(
                zstd::Encoder::new(BytesMut::new().writer(), i32::from(zstd_compression_level))
                    .expect("BytesMut has infallible IO"),
                body,
            ),
            Collate::Continued(encoder, body) => zstd_compress_fold_varint_frame(encoder, body),
        };
        encoder.flush().expect("BytesMut has infallible IO");
        let compressed_len = encoder.get_ref().get_ref().len();

        // eprintln!("Compressed length: {compressed_len}");

        match compressed_len >= zstd_frame_size_tripwire {
            // finish this zstd frame
            true => ControlFlow::Break(zstd_compress_finish(encoder)),
            // fold the next varint frame body in
            false => ControlFlow::Continue(encoder),
        }
    }
}

/// Encode `body` as a varint frame into `encoder` (writing the length and then the body itself)
/// ```text
///    ┌──────────────────────────────···
///    │ zstd frame
///    ├···┬───────────┬─────────────┬···
///    │   │varint:    │             │
///    │   │body length│frame body   │
///    └···┼───────────┴─────────────┼···
/// start ►│                    end ►│
/// ```
fn zstd_compress_fold_varint_frame(
    mut encoder: zstd::Encoder<Writer<BytesMut>>,
    body: BytesMut,
) -> zstd::Encoder<Writer<BytesMut>> {
    encoder
        .write_all(&body.len().encode_var_vec())
        .expect("BytesMut has infallible IO");
    encoder
        .write_all(&body)
        .expect("BytesMut has infallible IO");
    encoder
}

/// Finish a zstd frame
fn zstd_compress_finish(encoder: zstd::Encoder<Writer<BytesMut>>) -> BytesMut {
    let frame = encoder
        .finish()
        .expect("BytesMut has infallible IO")
        .into_inner();
    // eprintln!("Finished frame: {}", frame.len());
    frame
}

// #[allow(clippy::enum_variant_names)] // V2 support soon
// pub enum CarFormat<'index_writer> {
//     V1Plain,
//     /// See [crate::car_backed_blockstore::CompressedCarV1BackedBlockstore]
//     V1ManyFrame {
//         zstd_frame_size_tripwire: usize,
//         zstd_compression_level: u16,
//     },
//     V1ManyFrameIndexedOutOfBand {
//         zstd_frame_size_tripwire: usize,
//         zstd_compression_level: u16,
//         index_writer: Box<dyn AsyncWrite + 'index_writer>,
//     },
// }

// pub async fn write_car(
//     format: CarFormat<'_>,
//     roots: Vec<Cid>,
//     // TODO(aatifsyed): can we be smarter about the serialization here?
//     // TODO(aatifsyed): should this accept (Cid, Ipld)?
//     blocks: impl Stream<Item = io::Result<(Cid, Vec<u8>)>>,
//     // TODO(aatifsyed): document that this should be uncompressed for manyframe formats
//     writer: impl AsyncWrite,
// ) -> io::Result<()> {
//     match format {
//         CarFormat::V1Plain => {
//             stream::once(future::ready(Ok(uncompressed_v1_header(roots))))
//                 .chain(blocks.map_ok(|(cid, ipld)| concat_cid_and_block_data(cid, ipld)))
//                 .forward(FramedWrite::new(writer, VarintFrameCodec::default()))
//                 .await
//         }
//         CarFormat::V1ManyFrame {
//             zstd_frame_size_tripwire,
//             zstd_compression_level,
//         } => {
//             try_collate(
//                 stream::once(future::ready(Ok(uncompressed_v1_header(roots))))
//                     .chain(blocks.map_ok(|(cid, ipld)| concat_cid_and_block_data(cid, ipld))),
//                 varint_to_zstd_frame_collator(zstd_frame_size_tripwire, zstd_compression_level),
//                 zstd_compress_finish,
//             )
//             .forward(FramedWrite::new(writer, BytesCodec::new()))
//             .await
//         }
//         CarFormat::V1ManyFrameIndexedOutOfBand {
//             zstd_frame_size_tripwire,
//             zstd_compression_level,
//             index_writer,
//         } => {
//             let index = write_manyframe_and_create_index(
//                 roots,
//                 blocks,
//                 zstd_frame_size_tripwire,
//                 zstd_compression_level,
//                 writer,
//             )
//             .await?;

//             // TODO(aatifsyed): do we want a versioned index?
//             let mut serialized_index = BytesMut::new();
//             fvm_ipld_encoding::to_writer((&mut serialized_index).writer(), &index)
//                 .expect("BytesMut has infallible IO");
//             Box::into_pin(index_writer)
//                 .write_all_buf(&mut serialized_index)
//                 .await
//         }
//     }
// }

async fn write_manyframe_and_create_index(
    roots: Vec<Cid>,
    blocks: impl Stream<Item = io::Result<(Cid, Vec<u8>)>>,
    zstd_frame_size_tripwire: usize,
    zstd_compression_level: u16,
    writer: impl AsyncWrite,
) -> io::Result<ahash::HashMap<Cid, CompressedBlockDataLocation>> {
    let header = compressed_v1_header_varint(roots, zstd_compression_level);
    let mut zstd_frame_offset = u64::try_from(header.len()).unwrap();
    let mut index = ahash::HashMap::default();
    let zstd_frames = try_collate(
        blocks.map_ok(|(cid, ipld)| concat_cid_and_block_data(cid, ipld)),
        varint_to_zstd_frame_collator(zstd_frame_size_tripwire, zstd_compression_level),
        zstd_compress_finish,
    )
    .inspect_ok(|zstd_frame| {
        // TODO(aatifsyed): don't uncompress again
        let mut cursor = std::io::Cursor::new(
            zstd::decode_all(zstd_frame.as_ref()).expect("We've just compressed this frame"),
        );
        index.extend(
            iter::from_fn(|| {
                read_block_data_location_and_skip(&mut cursor)
                    .expect("we've just serialized this correctly, and BytesMut has infallible IO")
            })
            .map(|(cid, location_in_frame)| {
                (
                    cid,
                    CompressedBlockDataLocation {
                        zstd_frame_offset,
                        location_in_frame,
                    },
                )
            }),
        );
        // the next frame starts after the current one
        zstd_frame_offset += u64::try_from(zstd_frame.len()).unwrap();
    });
    stream::once(future::ready(Ok(header)))
        .chain(zstd_frames)
        .forward(FramedWrite::new(writer, BytesCodec::new()))
        .await?;
    Ok(index)
}

/// Suitable for placing into a varint frame
///
/// ```text
///  ┌──────────┐
///  │car header│
///  └──────────┘
/// ```
fn uncompressed_v1_header(roots: Vec<Cid>) -> BytesMut {
    let mut buffer = BytesMut::new();
    let header = CarHeader { roots, version: 1 };
    fvm_ipld_encoding::to_writer((&mut buffer).writer(), &header).expect(
        "BytesMut has infallible IO, and CarHeader probably doesn't validate on serialization",
    );
    buffer
}

/// Suitable for placing into a varint frame
///
/// ```text
///  ┌───┬──────────┐
///  │cid│block data│
///  └───┴──────────┘
/// ```
fn concat_cid_and_block_data(cid: Cid, ipld: Vec<u8>) -> BytesMut {
    let mut buffer = BytesMut::new();
    cid.write_bytes((&mut buffer).writer())
        .expect("BytesMut has infallible IO");
    buffer.extend(ipld);
    buffer
}

/// Store the header in its own varint frame, and compress it in a zstd frame
/// ```text
/// ┌──────────────────────┐
/// │ zstd frame           │
/// ├───────────┬──────────┤
/// │body length│car header│
/// └───────────┴──────────┘
/// ```
fn compressed_v1_header_varint(roots: Vec<Cid>, zstd_compression_level: u16) -> BytesMut {
    let mut compressor =
        zstd::Encoder::new(BytesMut::new().writer(), i32::from(zstd_compression_level))
            .expect("We're not using a dictionary");
    let header = CarHeader { roots, version: 1 };
    // we need the header length first
    let header = fvm_ipld_encoding::to_vec(&header).expect(
        "BytesMut has infallible IO, and CarHeader probably doesn't validate on serialization",
    );
    let mut len_buffer = unsigned_varint::encode::usize_buffer();
    let len = unsigned_varint::encode::usize(header.len(), &mut len_buffer);
    compressor
        .write_all(len)
        .and_then(|_| compressor.write_all(&header))
        .and_then(|_| compressor.finish())
        .expect("BytesMut has infallible IO")
        .into_inner()
}

#[cfg(test)]
mod tests {

    use super::{
        write_manyframe_and_create_index, zstd_compress_varint_manyframe,
        CompressedCarV1BackedBlockstore, UncompressedCarV1BackedBlockstore, ZstdFrames,
    };

    use cid::Cid;
    use futures::{
        executor::block_on,
        stream::{self, StreamExt as _},
    };
    use fvm_ipld_blockstore::{Blockstore as _, MemoryBlockstore};
    use fvm_ipld_car::{Block, CarReader};
    use tap::Tap as _;

    #[test]
    fn written_index_can_be_used_as_a_blockstore_index() {
        let (sample_roots, sample_data) = chain4_roots_and_contents();
        let mut many_frame_zstd = std::io::Cursor::new(vec![]);
        let index = block_on(write_manyframe_and_create_index(
            sample_roots.clone(),
            stream::iter(sample_data.clone()).map(Ok),
            8_000_usize.next_power_of_two(),
            3,
            &mut many_frame_zstd,
        ))
        .unwrap();
        many_frame_zstd.set_position(0);
        let blockstore_with_preindex = CompressedCarV1BackedBlockstore::new_with_trusted_index(
            many_frame_zstd.clone(),
            index,
            sample_roots,
        );
        let blockstore_without_preindex =
            CompressedCarV1BackedBlockstore::new(many_frame_zstd).unwrap();
        for cid in blockstore_without_preindex.cids() {
            let expected = sample_data.get(&cid).unwrap();
            assert_eq!(
                expected,
                &blockstore_with_preindex.get(&cid).unwrap().unwrap()
            );
            assert_eq!(
                expected,
                &blockstore_without_preindex.get(&cid).unwrap().unwrap()
            );
        }
    }

    #[test]
    fn test_uncompressed() {
        let car = chain4_car();
        let reference = reference(futures::io::Cursor::new(car));
        let car_backed = UncompressedCarV1BackedBlockstore::new(std::io::Cursor::new(car)).unwrap();

        assert_eq!(car_backed.cids().len(), 1222);
        assert_eq!(car_backed.roots().len(), 1);

        for cid in car_backed.cids() {
            let expected = reference.get(&cid).unwrap().unwrap();
            let actual = car_backed.get(&cid).unwrap().unwrap();
            assert_eq!(expected, actual);
        }
    }

    #[test]
    fn test_compressed_manyframe() {
        let car_manyframe = chain4_car_zstd_manyframe();
        let reference = reference(
            async_compression::futures::bufread::ZstdDecoder::new(car_manyframe.as_slice())
                .tap_mut(|it| it.multiple_members(true)),
        );
        let car_backed =
            CompressedCarV1BackedBlockstore::new(std::io::Cursor::new(car_manyframe)).unwrap();

        assert_eq!(car_backed.cids().len(), 1222);
        assert_eq!(car_backed.roots().len(), 1);

        for cid in car_backed.cids() {
            let expected = reference.get(&cid).unwrap().unwrap();
            let actual = car_backed.get(&cid).unwrap().unwrap();
            assert_eq!(expected, actual);
        }
    }

    fn reference(reader: impl futures::AsyncRead + Send + Unpin) -> MemoryBlockstore {
        block_on(async {
            let blockstore = MemoryBlockstore::new();
            let mut blocks = CarReader::new(reader).await.unwrap();
            while let Some(Block { cid, data }) = blocks.next_block().await.unwrap() {
                blockstore.put_keyed(&cid, &data).unwrap()
            }
            blockstore
        })
    }

    fn chain4_roots_and_contents() -> (Vec<Cid>, ahash::HashMap<Cid, Vec<u8>>) {
        let bs =
            UncompressedCarV1BackedBlockstore::new(std::io::Cursor::new(chain4_car())).unwrap();
        (
            bs.roots(),
            bs.cids()
                .into_iter()
                .map(|cid| (cid, bs.get(&cid).unwrap().unwrap()))
                .collect(),
        )
    }

    fn chain4_car() -> &'static [u8] {
        include_bytes!("../test-snapshots/chain4.car")
    }

    /// Don't clutter our repository with test .car files - just create one in-memory
    fn chain4_car_zstd_manyframe() -> Vec<u8> {
        let mut zstd_multiframe = vec![];

        let num_zstd_frames = block_on(zstd_compress_varint_manyframe(
            chain4_car(),
            &mut zstd_multiframe,
            8000usize.next_power_of_two(),
            3,
        ))
        .unwrap();
        assert_eq!(53, num_zstd_frames);

        zstd_multiframe
    }

    #[test]
    fn test_zstd_frames() {
        let frames = ZstdFrames::new(
            std::io::Cursor::new(chain4_car_zstd_manyframe()),
            1_000_000_u64.next_power_of_two(),
        )
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
        assert_eq!(53, frames.len());
    }

    #[test]
    fn test_manyframe_round_trip() {
        let round_tripped = zstd::decode_all(chain4_car_zstd_manyframe().as_slice()).unwrap();
        assert_eq!(round_tripped, chain4_car());
    }
}
