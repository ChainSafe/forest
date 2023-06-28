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
//! CARs consist of _frames_.
//! Each frame is a concatenation of the body length as an [`unsigned_varint`], and the _frame body_ itself.
//! [`unsigned_varint::codec`] can be used to read frames piecewise into memory.
//!
//! The first frame's body is a [`CarHeader`] encoded using [`ipld_dagcbor`](serde_ipld_dagcbor).
//!
//! Subsequent frame bodies are _blocks_, a concatenation of a [`Cid`] and binary `data`.
//!
//! The `offset` in [`BlockLocation`] is the offset of the frame body from the start of the file, illustrated below
//!
//! ```text
//! ┌──────────────┬──────────┐
//! │  frame       │          │
//! ├┬────────────┬┤          │
//! ││ length     ││          │
//! │┼────────────┼│          │
//! ││ body       ││          │
//! │┼┬──────────┬┼┼─┐        │
//! │││car header│││ │        │
//! ├┴┴──────────┴┴┤ ▼ length │
//! │  frame       │          │
//! ├┬────────────┬┤          │
//! ││ length     ││          │
//! │┼────────────┼│          │
//! ││ body       ││          │
//! │┼┬──────────┬┼┼─┐        ▼ offset
//! │││cid       │││ │
//! ││├──────────┤││ |
//! │││data      │││ |
//! ├┴┴──────────┴┴┤ ▼ length
//! │  frame...    │
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

use ahash::AHashMap;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_car::CarHeader;
use parking_lot::Mutex;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::io::{
    self,
    ErrorKind::{InvalidData, Other, UnexpectedEof, Unsupported},
    Read, Seek, SeekFrom,
};
use tracing::{debug, trace};

/// If you seek to `offset` (from the start of the file), and read `length` bytes,
/// you should get the concatenation of a [`Cid`] and its corresponding data.
///
/// See [module documentation](mod@self) for more.
#[derive(Debug)]
struct BlockLocation {
    offset: u64,
    length: usize,
}

/// See [module documentation](mod@self) for more.
// Theoretically, this should be clonable, with very low overhead
pub struct CarBackedBlockstore<ReaderT> {
    // Blockstore methods take `&self`, so lock here
    reader: Mutex<ReaderT>,
    write_cache: Mutex<AHashMap<Cid, Vec<u8>>>,
    index: AHashMap<Cid, BlockLocation>,
    pub roots: Vec<Cid>,
}

impl<ReaderT> CarBackedBlockstore<ReaderT>
where
    ReaderT: Read + Seek,
{
    /// To be correct:
    /// - `reader` must read immutable data. e.g if it is a file, it should be [`flock`](https://linux.die.net/man/2/flock)ed.
    ///   [`Blockstore`] API calls may panic if this is not upheld.
    /// - `reader`'s buffer should have enough room for the [`CarHeader`] and any [`Cid`]s.
    #[tracing::instrument(skip_all)]
    pub fn new(mut reader: ReaderT) -> cid::Result<Self> {
        let CarHeader { roots, version } = read_header(&mut reader)?;
        if version != 1 {
            return Err(cid::Error::Io(io::Error::new(
                Unsupported,
                "file must be CARv1",
            )));
        }

        // now create the index
        let index = std::iter::from_fn(|| read_block_location(&mut reader))
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
            reader: Mutex::new(reader),
            index,
            roots,
            write_cache: Mutex::new(AHashMap::new()),
        })
    }
}

impl<ReaderT> Blockstore for CarBackedBlockstore<ReaderT>
where
    ReaderT: Read + Seek,
{
    // This function should probably return a Cow<[u8]> to save unneccessary memcpys
    #[tracing::instrument(skip(self))]
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        match (self.index.get(k), self.write_cache.lock().entry(*k)) {
            (Some(_location), Occupied(cached)) => {
                trace!("evicting from write cache");
                Ok(Some(cached.remove()))
            }
            (Some(BlockLocation { offset, length }), Vacant(_)) => {
                trace!("fetching from disk");
                let mut reader = self.reader.lock();

                // Go to the start of the frame body, which is a concatenated CID and its data
                reader.seek(SeekFrom::Start(*offset))?;

                // Read the CID
                let cid = Cid::read_bytes(&mut *reader)?;
                assert_eq!(cid, *k);

                let cid_len = reader.stream_position()? - *offset;
                let data_len = *length - usize::try_from(cid_len).unwrap();
                let mut data = vec![0; data_len];
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
    // #[tracing::instrument(skip(self, block))]
    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        match (self.index.get(k), self.write_cache.lock().entry(*k)) {
            (Some(_), Occupied(_)) => {
                unreachable!("we never put a CID in the write cache if it exists on disk")
            }
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
        }
    }
}

#[tracing::instrument(skip_all, ret, err)]
fn read_header(reader: &mut impl Read) -> io::Result<CarHeader> {
    let header_len = read_usize(reader)?;
    let mut buffer = vec![0; header_len];
    reader.read_exact(&mut buffer)?;
    fvm_ipld_encoding::from_slice(&buffer).map_err(|e| io::Error::new(InvalidData, e))
}

// Importantly, we seek _past_ the data, rather than read any in.
// This allows us to keep indexing fast.
//
// Returns `Option<Result<..>>` so this function is suitable as a factory for a fused `TryStream`
#[tracing::instrument(skip_all, ret)]
fn read_block_location(
    reader: &mut (impl Read + Seek),
) -> Option<cid::Result<(Cid, BlockLocation)>> {
    // for our function signature
    macro_rules! ok {
        ($expr:expr) => {
            match $expr {
                Ok(inner) => inner,
                Err(err) => return Some(Err(err.into())),
            }
        };
    }
    let block_len = match read_usize(reader) {
        Ok(block_len) => block_len,
        // if the length itself is torn at an EOF, then we'll erroneously return None instead of Some(Err(_)).
        // but to detect this we either need
        // - a buffered reader, which is undesirable because block reads would then have a buffer
        //   indirection (or we'd have to juggle read buffers, which is error prone)
        // - a custom varint encoder API: `read_varint_or_eof(..) -> io::Result<Option<..>>`
        //
        // live with prematurely ending the block stream in this case.
        Err(e) if e.kind() == UnexpectedEof => return None,
        Err(other) => return Some(Err(cid::Error::Io(other))),
    };
    let block_offset = ok!(reader.stream_position());
    let cid = ok!(Cid::read_bytes(&mut *reader));
    let next_frame_offset = block_offset + u64::try_from(block_len).unwrap();
    ok!(reader.seek(SeekFrom::Start(next_frame_offset)));
    Some(Ok((
        cid,
        BlockLocation {
            offset: block_offset,
            length: block_len,
        },
    )))
}

fn read_usize(reader: &mut impl Read) -> io::Result<usize> {
    use unsigned_varint::io::ReadError::{Decode, Io};
    match unsigned_varint::io::read_usize(reader) {
        Ok(u) => Ok(u),
        Err(Io(e)) => Err(e),
        Err(Decode(e)) => Err(io::Error::new(InvalidData, e)),
        Err(other) => Err(io::Error::new(Other, other)),
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

        assert_eq!(car_backed.index.len(), 1222);
        assert_eq!(car_backed.roots.len(), 1);

        for cid in car_backed.index.keys() {
            let expected = reference.get(cid).unwrap().unwrap();
            let actual = car_backed.get(cid).unwrap().unwrap();
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
