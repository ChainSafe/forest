// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! It can often be time, memory, or disk prohibitive to read large snapshots into a database like [`ParityDb`](crate::db::parity_db::ParityDb).
//!
//! This module provides an implementer of [`Blockstore`] that simply wraps a [CAR file](https://ipld.io/specs/transport/car/carv1).
//! **Note that all operations on this store are blocking**.
//!
//! On creation, [`CarBackedBlockstore`] builds an in-memory index of the [`Cid`]s in the file,
//! and their offsets into that file.
//!
//! When a block is requested [`CarBackedBlockstore`] scrolls to that offset, and reads the block, on-demand.
//!
//! Writes for new data (which doesn't exist in the CAR already) are currently cached in-memory.
//!
//! Random-access performance is expected to be poor, as the OS will have to load separate parts of the file from disk, and flush it for each read.
//! However, (near) linear access should be pretty good, as file chunks will be pre-fetched.
//! See also the remarks below about block ordering.
//!
//! # CAR Layout and seeking
//!
//! CARs consist of _varint frames_, are a concatenation of the _body length_ as an [`unsigned_varint`], and the _frame body_ itself.
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
//!
//! - [CAR documentation](https://ipld.io/specs/transport/car/carv1/#determinism)
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
//! - Theoretically, [`CarBackedBlockstore`] should be clonable (or even [`Sync`]) with very low overhead, so that multiple threads could perform operations concurrently.

use ahash::AHashMap;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_car::CarHeader;
use parking_lot::Mutex;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::io::{
    self, BufReader,
    ErrorKind::{InvalidData, Other, UnexpectedEof, Unsupported},
    Read, Seek, SeekFrom,
};
use tracing::{debug, trace};

/// If you seek to `offset` (from the start of the file), and read `length` bytes,
/// you should get data that corresponds to a [`Cid`] (but NOT the [`Cid`] itself).
#[derive(Debug)]
struct BlockDataLocation {
    offset: u64,
    length: u32,
}

/// See [module documentation](mod@self) for more.
pub struct CarBackedBlockstore<ReaderT> {
    // https://github.com/ChainSafe/forest/issues/3096
    inner: Mutex<CarBackedBlockstoreInner<ReaderT>>,
}

impl<ReaderT> CarBackedBlockstore<ReaderT>
where
    ReaderT: Read + Seek,
{
    pub fn roots(&self) -> Vec<Cid> {
        self.inner.lock().roots.clone()
    }

    /// To be correct:
    /// - `reader` must read immutable data. e.g if it is a file, it should be [`flock`](https://linux.die.net/man/2/flock)ed.
    ///   [`Blockstore`] API calls may panic if this is not upheld.
    /// - `reader`'s buffer should have enough room for the [`CarHeader`] and any [`Cid`]s.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn new(mut reader: ReaderT) -> cid::Result<Self> {
        let CarHeader { roots, version } = read_header(&mut reader)?;
        if version != 1 {
            return Err(cid::Error::Io(io::Error::new(
                Unsupported,
                "file must be CARv1",
            )));
        }

        // When indexing, we perform small reads of the length and CID before seeking
        // A small buffer helps ~5% (n=1)
        let mut buf_reader = BufReader::with_capacity(128, reader);

        // now create the index
        let index = std::iter::from_fn(|| read_block_location_or_eof(&mut buf_reader).transpose())
            .collect::<Result<AHashMap<_, _>, _>>()?;
        match index.len() {
            0 => {
                return Err(cid::Error::Io(io::Error::new(
                    InvalidData,
                    "CARv1 files must not be empty",
                )))
            }
            num_blocks => debug!(num_blocks, "indexed CAR"),
        }

        Ok(Self {
            inner: Mutex::new(CarBackedBlockstoreInner {
                // discarding the buffer is ok - we only seek within this now
                reader: buf_reader.into_inner(),
                index,
                roots,
                write_cache: AHashMap::new(),
            }),
        })
    }
}

struct CarBackedBlockstoreInner<ReaderT> {
    reader: ReaderT,
    write_cache: AHashMap<Cid, Vec<u8>>,
    index: AHashMap<Cid, BlockDataLocation>,
    roots: Vec<Cid>,
}

impl<ReaderT> Blockstore for CarBackedBlockstore<ReaderT>
where
    ReaderT: Read + Seek,
{
    #[tracing::instrument(level = "trace", skip(self))]
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        let CarBackedBlockstoreInner {
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
            (Some(BlockDataLocation { offset, length }), Vacant(_)) => {
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
        let CarBackedBlockstoreInner {
            write_cache, index, ..
        } = &mut *self.inner.lock();
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
}

#[tracing::instrument(level = "trace", skip_all, ret, err)]
fn read_header(mut reader: impl Read) -> io::Result<CarHeader> {
    let header_len = read_u32_or_eof(&mut reader)?.ok_or(io::Error::from(UnexpectedEof))?;
    let mut buffer = vec![0; usize::try_from(header_len).unwrap()];
    reader.read_exact(&mut buffer)?;
    fvm_ipld_encoding::from_slice(&buffer).map_err(|e| io::Error::new(InvalidData, e))
}

/// Importantly, we seek _past_ the data, rather than read any in.
/// This allows us to keep indexing fast.
///
/// [`Ok(None)`] on EOF
#[tracing::instrument(level = "trace", skip_all, ret)]
fn read_block_location_or_eof(
    mut reader: (impl Read + Seek),
) -> cid::Result<Option<(Cid, BlockDataLocation)>> {
    let Some((frame_body_offset, body_length)) = next_varint_frame(&mut reader)? else {
        return Ok(None)
    };
    let cid = Cid::read_bytes(&mut reader)?;
    // tradeoff: we perform a second syscall here instead of in Blockstore::get,
    // and keep BlockDataLocation purely for the blockdata
    let block_data_offset = reader.stream_position()?;
    let next_frame_offset = frame_body_offset + u64::from(body_length);
    let block_data_length = u32::try_from(next_frame_offset - block_data_offset).unwrap();
    reader.seek(SeekFrom::Start(next_frame_offset))?;
    Ok(Some((
        cid,
        BlockDataLocation {
            offset: block_data_offset,
            length: block_data_length,
        },
    )))
}

fn next_varint_frame(mut reader: (impl Read + Seek)) -> io::Result<Option<(u64, u32)>> {
    Ok(match read_u32_or_eof(&mut reader)? {
        Some(body_length) => {
            let frame_body_offset = reader.stream_position()?;
            Some((frame_body_offset, body_length))
        }
        None => None,
    })
}

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

#[cfg(test)]
mod tests {
    use super::CarBackedBlockstore;

    use futures_util::AsyncRead;
    use fvm_ipld_blockstore::{Blockstore as _, MemoryBlockstore};
    use fvm_ipld_car::{Block, CarReader};

    #[test]
    fn test() {
        let car = include_bytes!("../test-snapshots/chain4.car");
        let reference = reference(futures::io::Cursor::new(car));
        let car_backed = CarBackedBlockstore::new(std::io::Cursor::new(car)).unwrap();

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
