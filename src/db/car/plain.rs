// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! # Varint frames
//!
//! CARs are made of concatenations of _varint frames_. Each varint frame is a concatenation of the
//! _body length_ as an
//! [varint](https://docs.rs/integer-encoding/4.0.0/integer_encoding/trait.VarInt.html), and the
//! _frame body_ itself. [`crate::utils::encoding::uvibytes::UviBytes`] can be used to read frames
//! piecewise into memory.
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
//! The first varint frame is a _header frame_, where the frame body is a [`CarHeader`] encoded
//! using [`ipld_dagcbor`](serde_ipld_dagcbor).
//!
//! Subsequent varint frames are _block frames_, where the frame body is a concatenation of a
//! [`Cid`] and the _block data_ addressed by that CID.
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
//! > _... a filecoin-deterministic car-file is currently implementation-defined as containing all
//! > DAG-forming blocks in first-seen order, as a result of a depth-first DAG traversal starting
//! > from a single root._
//! - [CAR documentation](https://ipld.io/specs/transport/car/carv1/#determinism)
//!
//! # Future work
//! - [`fadvise`](https://linux.die.net/man/2/posix_fadvise)-based APIs to pre-fetch parts of the
//!   file, to improve random access performance.
//! - Use an inner [`Blockstore`] for writes.
//! - Use safe arithmetic for all operations - a malicious frame shouldn't cause a crash.
//! - Theoretically, file-backed blockstores should be clonable (or even [`Sync`]) with very low
//!   overhead, so that multiple threads could perform operations concurrently.
//! - CARv2 support
//! - A wrapper that abstracts over car formats for reading.

use crate::blocks::{Tipset, TipsetKeys};
use ahash::HashMapExt as _;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_car::CarHeader;
use integer_encoding::VarIntReader;
use parking_lot::Mutex;
use std::{
    any::Any,
    collections::hash_map::Entry::{Occupied, Vacant},
    io::{
        self, BufReader,
        ErrorKind::{InvalidData, UnexpectedEof, Unsupported},
        Read, Seek, SeekFrom,
    },
    iter,
};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tracing::{debug, trace};

/// **Note that all operations on this store are blocking**.
///
/// It can often be time, memory, or disk prohibitive to read large snapshots into a database like
/// [`ParityDb`](crate::db::parity_db::ParityDb).
///
/// This is an implementer of [`Blockstore`] that simply wraps an uncompressed [CARv1
/// file](https://ipld.io/specs/transport/car/carv1).
///
/// On creation, [`PlainCar`] builds an in-memory index of the [`Cid`]s in the file,
/// and their offsets into that file.
/// Note that it prepares its own buffer for doing so.
///
/// When a block is requested, [`PlainCar`] scrolls to that offset, and reads the block, on-demand.
///
/// Writes for new blocks (which don't exist in the CAR already) are currently cached in-memory.
///
/// Random-access performance is expected to be poor, as the OS will have to load separate parts of
/// the file from disk, and flush it for each read. However, (near) linear access should be pretty
/// good, as file chunks will be pre-fetched.
///
/// See [module documentation](mod@self) for more.
pub struct PlainCar<ReaderT> {
    // https://github.com/ChainSafe/forest/issues/3096
    inner: Mutex<PlainCarInner<ReaderT>>,
}

impl<ReaderT: super::CarReader> PlainCar<ReaderT> {
    /// To be correct:
    /// - `reader` must read immutable data. e.g if it is a file, it should be
    ///   [`flock`](https://linux.die.net/man/2/flock)ed.
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
                    inner: Mutex::new(PlainCarInner {
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

    pub fn heaviest_tipset(&self) -> anyhow::Result<Tipset> {
        Tipset::load_required(self, &TipsetKeys::new(self.roots()))
    }

    /// In an arbitrary order
    #[cfg(test)]
    pub fn cids(&self) -> Vec<Cid> {
        self.inner.lock().index.keys().cloned().collect()
    }

    pub fn into_dyn(self) -> PlainCar<Box<dyn super::CarReader>> {
        let PlainCarInner {
            reader,
            write_cache,
            index,
            roots,
        } = self.inner.into_inner();
        PlainCar {
            inner: Mutex::new(PlainCarInner {
                reader: Box::new(reader),
                write_cache,
                index,
                roots,
            }),
        }
    }
}

impl TryFrom<&'static [u8]> for PlainCar<std::io::Cursor<&'static [u8]>> {
    type Error = io::Error;
    fn try_from(bytes: &'static [u8]) -> io::Result<Self> {
        PlainCar::new(std::io::Cursor::new(bytes))
    }
}

struct PlainCarInner<ReaderT> {
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

impl<ReaderT> Blockstore for PlainCar<ReaderT>
where
    ReaderT: Read + Seek,
{
    #[tracing::instrument(level = "trace", skip(self))]
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        let PlainCarInner {
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
        let PlainCarInner {
            write_cache, index, ..
        } = &mut *self.inner.lock();
        handle_write_cache(write_cache, index, k, block)
    }
}

pub async fn write_skip_frame_header_async(
    writer: &mut (impl AsyncWrite + Unpin),
    data_len: u32,
) -> std::io::Result<()> {
    writer.write_all(&[0x50, 0x2A, 0x4D, 0x18]).await?;
    writer.write_all(&data_len.to_le_bytes()).await?;
    Ok(())
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CompressedBlockDataLocation {
    pub zstd_frame_offset: u64,
    pub location_in_frame: UncompressedBlockDataLocation,
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
#[tracing::instrument(level = "trace", skip_all, ret)]
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

#[cfg(test)]
mod tests {

    use super::PlainCar;

    use futures::executor::block_on;
    use fvm_ipld_blockstore::{Blockstore as _, MemoryBlockstore};
    use fvm_ipld_car::{Block, CarReader};

    #[test]
    fn test_uncompressed() {
        let car = chain4_car();
        let reference = reference(futures::io::Cursor::new(car));
        let car_backed = PlainCar::new(std::io::Cursor::new(car)).unwrap();

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

    fn chain4_car() -> &'static [u8] {
        include_bytes!("../../../test-snapshots/chain4.car")
    }
}
