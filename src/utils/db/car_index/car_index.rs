// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::{BlockPosition, Hash, IndexHeader, Slot};
use cid::Cid;
use smallvec::{smallvec, SmallVec};
use std::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom};

pub struct CarIndex<ReaderT> {
    reader: ReaderT,
    offset: u64,
    header: IndexHeader,
}

impl<ReaderT: Read + Seek> CarIndex<ReaderT> {
    /// `O(1)` Open a reader as a mapping from cids to block positions in a
    /// content-addressable archive.
    pub fn open(mut reader: ReaderT, offset: u64) -> Result<Self> {
        reader.seek(SeekFrom::Start(offset))?;
        let header = IndexHeader::read(&mut reader)?;
        if header.magic_number != IndexHeader::MAGIC_NUMBER {
            Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Invalid magic number: {:x}. Expected: {:x}",
                    header.magic_number,
                    IndexHeader::MAGIC_NUMBER
                ),
            ))
        } else {
            Ok(CarIndex {
                reader,
                offset: offset + IndexHeader::SIZE as u64,
                header,
            })
        }
    }

    /// `O(1)` Look up possible `BlockPosition`s for a `Cid`. Does not allocate
    /// unless 2 or more cids have collided.
    pub fn lookup(&mut self, key: Cid) -> Result<SmallVec<[BlockPosition; 1]>> {
        self.lookup_internal(Hash::from(key))
    }

    #[cfg(any(test, feature = "benchmark-private"))]
    pub fn lookup_hash(&mut self, hash: Hash) -> Result<SmallVec<[BlockPosition; 1]>> {
        self.lookup_internal(hash)
    }

    // Jump to bucket offset and scan downstream. All key-value pairs with the
    // right key are guaranteed to appear before we encounter an empty slot.
    fn lookup_internal(&mut self, hash: Hash) -> Result<SmallVec<[BlockPosition; 1]>> {
        self.reader.seek(SeekFrom::Start(
            self.offset + hash.bucket(self.header.buckets) * Slot::SIZE as u64,
        ))?;
        while let Slot::Full(entry) = Slot::read(&mut self.reader)? {
            if entry.hash == hash {
                let mut ret = smallvec![entry.value];
                // The entries are sorted. Once we've found a matching
                // key, all duplicate hash keys will be right next to
                // it. Note that it's extremely rare for hashes to
                // collide.
                while let Some(value) = Slot::read_with_hash(&mut self.reader, hash)? {
                    ret.push(value);
                }
                return Ok(ret);
            }
        }
        Ok(smallvec![])
    }
}
