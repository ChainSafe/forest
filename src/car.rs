// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! # Varint frames
//!
//! CARs are made of concatenations of _varint frames_.
//! Each varint frame is a concatenation of the _body length_ as an [`unsigned_varint`], and the _frame body_ itself.
//! [`unsigned_varint::codec`] can be used to read frames piecewise into memory.
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
//! # Future work
//! - [`fadvise`](https://linux.die.net/man/2/posix_fadvise)-based APIs to pre-fetch parts of the file, to improve random access performance.
//! - Use an inner [`Blockstore`] for writes.
//! - Support compressed snapshots.
//!   Note that [`zstd`](https://github.com/facebook/zstd/blob/e4aeaebc201ba49fec50b087aeb15343c63712e5/doc/zstd_compression_format.md#zstandard-frames) archives are also composed of frames.
//!   Snapshots typically comprise of a single frame, but that would require decompressing all preceding data, precluding random access.
//!   So compressed snapshot support would compression per-frame, or maybe per block of frames.
//! - Support multiple files by concatenating them.
//! - Use safe arithmetic for all operations - a malicious frame shouldn't cause a crash.
//! - Theoretically, file-backed blockstores should be clonable (or even [`Sync`]) with very low overhead, so that multiple threads could perform operations concurrently.

use ahash::AHashMap;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_car::CarHeader;
use parking_lot::Mutex;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::io::BufRead;
use std::io::{
    self, BufReader,
    ErrorKind::{InvalidData, Other, UnexpectedEof, Unsupported},
    Read, Seek, SeekFrom,
};
use tracing::{debug, trace};

/// If you seek to `offset` (from the start of the file), and read `length` bytes,
/// you should get data that corresponds to a [`Cid`] (but NOT the [`Cid`] itself).
#[derive(Debug)]
struct UncompressedBlockDataLocation {
    offset: u64,
    length: u32,
}

/// It can often be time, memory, or disk prohibitive to read large snapshots into a database like [`ParityDb`](crate::db::parity_db::ParityDb).
///
/// This is an implementer of [`Blockstore`] that simply wraps an uncompressed [CARv1 file](https://ipld.io/specs/transport/car/carv1).
/// **Note that all operations on this store are blocking**.
///
/// On creation, [`UncompressedCarV1BackedBlockstore`] builds an in-memory index of the [`Cid`]s in the file,
/// and their offsets into that file.
///
/// When a block is requested [`UncompressedCarV1BackedBlockstore`] scrolls to that offset, and reads the block, on-demand.
///
/// Writes for new data (which doesn't exist in the CAR already) are currently cached in-memory.
///
/// Random-access performance is expected to be poor, as the OS will have to load separate parts of the file from disk, and flush it for each read.
/// However, (near) linear access should be pretty good, as file chunks will be pre-fetched.
///
/// See [module documentation](mod@self) for more.
///
/// ## Block ordering
/// > _... a filecoin-deterministic car-file is currently implementation-defined as containing all DAG-forming blocks in first-seen order, as a result of a depth-first DAG traversal starting from a single root._
///
/// - [CAR documentation](https://ipld.io/specs/transport/car/carv1/#determinism)
pub struct UncompressedCarV1BackedBlockstore<ReaderT> {
    // https://github.com/ChainSafe/forest/issues/3096
    inner: Mutex<UncompressedCarV1BackedBlockstoreInner<ReaderT>>,
}

impl<ReaderT> UncompressedCarV1BackedBlockstore<ReaderT>
where
    ReaderT: Read + Seek,
{
    pub fn roots(&self) -> Vec<Cid> {
        self.inner.lock().roots.clone()
    }

    /// To be correct:
    /// - `reader` must read immutable data. e.g if it is a file, it should be [`flock`](https://linux.die.net/man/2/flock)ed.
    ///   [`Blockstore`] API calls may panic if this is not upheld.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn new(mut reader: ReaderT) -> cid::Result<Self> {
        let roots = get_roots_from_v1_header(&mut reader)?;

        // When indexing, we perform small reads of the length and CID before seeking
        // Buffering these gives us a ~50% speedup (n=10): https://github.com/ChainSafe/forest/pull/3085#discussion_r1246897333
        let mut buf_reader = BufReader::with_capacity(1024, reader);

        // now create the index
        let index =
            std::iter::from_fn(|| read_block_location_and_skip(&mut buf_reader).transpose())
                .collect::<Result<AHashMap<_, _>, _>>()?;

        match index.len() {
            0 => Err(cid::Error::Io(io::Error::new(
                InvalidData,
                "CARv1 files must not be empty",
            ))),
            num_blocks => {
                debug!(num_blocks, "indexed CAR");
                Ok(Self {
                    inner: Mutex::new(UncompressedCarV1BackedBlockstoreInner {
                        // discarding the buffer is ok - we only seek within this now
                        reader: buf_reader.into_inner(),
                        index,
                        roots,
                        write_cache: AHashMap::new(),
                    }),
                })
            }
        }
    }
}

struct UncompressedCarV1BackedBlockstoreInner<ReaderT> {
    reader: ReaderT,
    write_cache: AHashMap<Cid, Vec<u8>>,
    index: AHashMap<Cid, UncompressedBlockDataLocation>,
    roots: Vec<Cid>,
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
    /// - If the write cache contains different data with this CID
    /// - See also [`Self::new`].
    #[tracing::instrument(level = "trace", skip(self, block))]
    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        let UncompressedCarV1BackedBlockstoreInner {
            write_cache, index, ..
        } = &mut *self.inner.lock();
        handle_write_cache(write_cache, index, k, block)
    }
}

fn handle_write_cache<LocationT>(
    write_cache: &mut AHashMap<Cid, Vec<u8>>,
    index: &mut AHashMap<Cid, LocationT>,
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
            unreachable!("we don't a CID in the write cache if it exists on disk")
        }
    }
}

pub struct CompressedCarV1BackedBlockstore<ReaderT> {
    // https://github.com/ChainSafe/forest/issues/3096
    inner: Mutex<CompressedCarV1BackedBlockstoreInner<ReaderT>>,
}

struct CompressedCarV1BackedBlockstoreInner<ReaderT> {
    reader: ReaderT,
    write_cache: AHashMap<Cid, Vec<u8>>,
    // Cid -> zstd frame offset
    index: AHashMap<Cid, u64>,
    roots: Vec<Cid>,
    // skip the header where appropriate
    first_frame_offset: u64,
}

impl<ReaderT> CompressedCarV1BackedBlockstore<ReaderT> {
    pub fn roots(&self) -> Vec<Cid> {
        self.inner.lock().roots.clone()
    }

    /// To be correct:
    /// - `reader` must read immutable data. e.g if it is a file, it should be [`flock`](https://linux.die.net/man/2/flock)ed.
    ///   [`Blockstore`] API calls may panic if this is not upheld.
    // This code is ugly because we have to recreate the decoder after every zstd frame.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn new(mut reader: ReaderT) -> cid::Result<Self>
    where
        ReaderT: BufRead + Seek,
    {
        let mut block_buffer = vec![];
        let mut index = AHashMap::new();

        // read the first zstd frame, it should contain a header
        let first_frame_offset = reader.stream_position()?;
        let mut decoder = zstd::Decoder::with_buffer(&mut reader)?.single_frame();
        let roots = get_roots_from_v1_header(&mut decoder)?;
        index.extend(
            read_cids(&mut decoder, &mut block_buffer)?
                .into_iter()
                .map(|cid| (cid, first_frame_offset)),
        );

        loop {
            if reader.fill_buf()?.is_empty() {
                let num_blocks = index.len();
                debug!(num_blocks, "indexed CAR");
                break Ok(Self {
                    inner: Mutex::new(CompressedCarV1BackedBlockstoreInner {
                        reader,
                        write_cache: AHashMap::new(),
                        index,
                        roots,
                        first_frame_offset,
                    }),
                });
            }
            let zstd_frame_offset = reader.stream_position()?;
            let mut decoder = zstd::Decoder::with_buffer(&mut reader)?.single_frame();
            index.extend(
                read_cids(&mut decoder, &mut block_buffer)?
                    .into_iter()
                    .map(|cid| (cid, zstd_frame_offset)),
            );
        }
    }
}

/// `f` is a callback that takes `zstd_frame_offset_position` and an `impl Read`.
/// It is run for each zstd frame.
// It's surprisingly hard to keep track of zstd frames and recreate the decoder for the shared reader...
// So take a callback instead
fn for_each_zstd_frame<ReaderT, F>(mut reader: ReaderT, mut f: F) -> io::Result<usize>
where
    ReaderT: BufRead + Seek,
    F: FnMut(u64, &'_ mut zstd::Decoder<'_, &'_ mut ReaderT>) -> io::Result<()>,
{
    let mut num_frames = 0;
    loop {
        if reader.fill_buf()?.is_empty() {
            break Ok(num_frames);
        }
        num_frames += 1;
        let stream_position = reader.stream_position()?;
        let mut uncompressed = zstd::Decoder::with_buffer(&mut reader)?.single_frame();
        f(stream_position, &mut uncompressed)?;
        let _leftover_bytes_in_frame = std::io::copy(&mut uncompressed, &mut std::io::sink())?;
    }
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
            first_frame_offset,
            ..
        } = &mut *self.inner.lock();
        match (index.get(k), write_cache.entry(*k)) {
            (Some(_location), Occupied(cached)) => {
                trace!("evicting from write cache");
                Ok(Some(cached.remove()))
            }
            (Some(zstd_frame_offset), Vacant(_)) => {
                trace!(zstd_frame_offset, "fetching from disk");
                reader.seek(SeekFrom::Start(*zstd_frame_offset))?;
                let mut uncompressed = zstd::Decoder::new(reader)?.single_frame();
                let mut block_data = vec![];
                if zstd_frame_offset == first_frame_offset {
                    read_header(&mut uncompressed)?;
                }
                std::iter::from_fn(|| {
                    copy_varint_framed_block(&mut uncompressed, &mut block_data)
                        .expect("invalid index: zstd frame doesn't contain valid blocks")
                })
                .find(|it| it == k)
                .expect("invalid index: zstd frame doesn't contain block");
                Ok(Some(block_data))
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

fn get_roots_from_v1_header(reader: impl Read) -> io::Result<Vec<Cid>> {
    match read_header(reader)? {
        CarHeader { roots, version: 1 } if !roots.is_empty() => Ok(roots),
        _other_version => Err(io::Error::new(
            Unsupported,
            "file must be CARv1 with non-empty roots",
        )),
    }
}

#[tracing::instrument(level = "trace", skip_all, ret, err)]
fn read_header(mut reader: impl Read) -> io::Result<CarHeader> {
    let header_len = read_u32_or_eof(&mut reader)?.ok_or(io::Error::from(UnexpectedEof))?;
    let mut buffer = vec![0; usize::try_from(header_len).unwrap()];
    reader.read_exact(&mut buffer)?;
    fvm_ipld_encoding::from_slice(&buffer).map_err(|e| io::Error::new(InvalidData, e))
}

#[tracing::instrument(level = "trace", skip_all, err)]
fn read_cids(mut reader: impl Read, buffer: &mut Vec<u8>) -> cid::Result<Vec<Cid>> {
    let mut cids = vec![];
    while let Some(body_length) = read_u32_or_eof(&mut reader)? {
        buffer.resize(usize::try_from(body_length).unwrap(), 0);
        reader.read_exact(buffer)?;
        let cid = Cid::read_bytes(buffer.as_slice())?;
        cids.push(cid)
    }
    Ok(cids)
}

/// Returns ([`Cid`], the `block data offset` and `block data length`)
/// ```text
/// start ►│                     end ►│
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
///
/// TODO(aatifsyed): is the speed claim even true? could we use [`copy_varint_framed_block`] instead?
#[tracing::instrument(level = "trace", skip_all, ret)]
fn read_block_location_and_skip(
    mut reader: (impl Read + Seek),
) -> cid::Result<Option<(Cid, UncompressedBlockDataLocation)>> {
    let Some(body_length) = read_u32_or_eof(&mut reader)? else {
        return Ok(None);
    };
    let frame_body_offset = reader.stream_position()?;
    let mut counted_reader = CountRead::new(&mut reader);
    let cid = Cid::read_bytes(&mut counted_reader)?;

    // counting the read bytes saves us a syscall
    let cid_length = counted_reader.count;
    let block_data_offset = frame_body_offset + u64::try_from(cid_length).unwrap();
    let next_frame_offset = frame_body_offset + u64::from(body_length);
    let block_data_length = u32::try_from(next_frame_offset - block_data_offset).unwrap();
    reader.seek(SeekFrom::Start(next_frame_offset))?;
    Ok(Some((
        cid,
        UncompressedBlockDataLocation {
            offset: block_data_offset,
            length: block_data_length,
        },
    )))
}

/// Returns `cid` and copies `block data` into the buffer provided,
/// leaving the reader at the marked position or returns [`Ok(None)`] at EOF
/// ```text
/// start ►│
///        ├───────────┬───┬──────────┐
///        │body length│cid│block data│
///        └───────────┴───┴──────────┤
///                              end ►│
/// ```
#[tracing::instrument(level = "trace", skip_all, ret, err)]
fn copy_varint_framed_block(
    mut reader: impl Read,
    block_data: &mut Vec<u8>,
) -> cid::Result<Option<Cid>> {
    let Some(body_length) = read_u32_or_eof(&mut reader)? else {
        return Ok(None)
    };
    let mut reader = CountRead::new(reader);
    let cid = Cid::read_bytes(&mut reader)?;
    let block_data_length = usize::try_from(body_length).unwrap() - reader.bytes_read();
    block_data.resize(block_data_length, 0);
    reader.read_exact(block_data)?;
    Ok(Some(cid))
}

/// Reads `body length`, leaving the reader at the marked position,
/// or returns [`Ok(None)`] if we've reached EOF
/// ```text
/// start ►│
///        ├───────────┬─────────────┐
///        │varint:    │             │
///        │body length│frame body   │
///        └───────────┼─────────────┘
///               end ►│
/// ```
fn read_u32_or_eof(mut reader: impl Read) -> io::Result<Option<u32>> {
    use unsigned_varint::io::{
        read_u32,
        ReadError::{Decode, Io},
    };

    let mut byte = [0u8; 1]; // detect EOF
    match reader.read(&mut byte)? {
        0 => Ok(None),
        1 => read_u32(byte.chain(reader))
            .map_err(|varint_error| match varint_error {
                Io(e) => e,
                Decode(e) => io::Error::new(InvalidData, e),
                other => io::Error::new(Other, other),
            })
            .map(Some),
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

#[cfg(test)]
mod tests {
    use super::{CompressedCarV1BackedBlockstore, UncompressedCarV1BackedBlockstore};

    use futures_util::AsyncRead;
    use fvm_ipld_blockstore::{Blockstore as _, MemoryBlockstore};
    use fvm_ipld_car::{Block, CarReader};
    use tap::Tap;

    #[test]
    fn test_uncompressed() {
        let car = include_bytes!("../test-snapshots/chain4.car");
        let reference = reference(futures::io::Cursor::new(car));
        let car_backed = UncompressedCarV1BackedBlockstore::new(std::io::Cursor::new(car)).unwrap();

        assert_eq!(car_backed.inner.lock().index.len(), 1222);
        assert_eq!(car_backed.inner.lock().roots.len(), 1);

        let cids = {
            let holding_lock = car_backed.inner.lock();
            holding_lock.index.keys().cloned().collect::<Vec<_>>()
        };

        for cid in cids {
            let expected = reference.get(&cid).unwrap().unwrap();
            let actual = car_backed.get(&cid).unwrap().unwrap();
            assert_eq!(expected, actual);
        }
    }

    #[test]
    fn test_compressed_manyframe() {
        let car = include_bytes!("../test-snapshots/chain4.car.zst-manyframe");
        let reference = reference(
            async_compression::futures::bufread::ZstdDecoder::new(futures::io::Cursor::new(car))
                .tap_mut(|it| it.multiple_members(true)),
        );
        let car_backed = CompressedCarV1BackedBlockstore::new(std::io::Cursor::new(car)).unwrap();

        assert_eq!(car_backed.inner.lock().index.len(), 1222);
        assert_eq!(car_backed.inner.lock().roots.len(), 1);

        let cids = {
            let holding_lock = car_backed.inner.lock();
            holding_lock.index.keys().cloned().collect::<Vec<_>>()
        };

        for cid in cids {
            let expected = reference.get(&cid).unwrap().unwrap();
            let actual = car_backed.get(&cid).unwrap().unwrap();
            assert_eq!(expected, actual);
        }
    }

    fn reference(reader: impl AsyncRead + Send + Unpin) -> MemoryBlockstore {
        futures::executor::block_on(async {
            let blockstore = MemoryBlockstore::new();
            let mut blocks = CarReader::new(reader).await.unwrap();
            while let Some(Block { cid, data }) = blocks.next_block().await.unwrap() {
                blockstore.put_keyed(&cid, &data).unwrap()
            }
            blockstore
        })
    }
}
