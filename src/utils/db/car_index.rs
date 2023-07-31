// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! # TL;DR
//!
//! [`CarIndex`] is equivalent to `HashMap<Cid, Vec<FrameOffset>>`. It can be
//! built in `O(n)` time, loaded from a reader in `O(1)` time, has `O(1)`
//! queries, and uses no caches by default.
//!
//! # Context: Binary search
//!
//! You can find any word in a dictionary in `O(logn)` steps by binary search.
//! However, no human would look for `aardvark` in the middle of a dictionary.
//! It's reasonable to assume that a word starting with two `A`s will appear
//! closer to the beginning of the dictionary rather than the end.
//!
//! Dictionary words are not evenly distributed, though, as there are more words
//! starting with `c` than with `a`. This makes it difficult to know exactly
//! where to start looking for a word in the dictionary.
//!
//! If we don't care about ordering, we can hash the words and thus be sure that
//! they are spread evenly across the dictionary. No hash key is any more or
//! less likely than any other.
//!
//! # Context: Hash tables and linear probing
//!
//! We could take the dictionary words and put them in different buckets
//! according to their hash value. Looking up a word requires finding the right
//! bucket and sifting through its content. With more buckets and words, each
//! bucket will, on average, not contain more than a single word.
//!
//! Let's simplify things and put the buckets in a single array. The contents of
//! bucket 0 starts at offset 0, bucket 1 starts at offset 1, etc. Some buckets
//! are empty and leave unfilled slots, other buckets have multiple entries and
//! spill into slots meant for someone else.
//!
//! To look up a value in this array, we go to the bucket offset and scan
//! towards the end of the array. The scan ends once we've found the key or an
//! empty slot. The maximum number of elements to scan is guaranteed to be
//! small.
//!
//! This technique is called [linear
//! probing](https://en.wikipedia.org/wiki/Linear_probing).
//!
//! ## Hash collisions and bucket collisions
//!
//! It may happen that two keys have the same hash. In this (extremely rare)
//! case, looking up the key will return two values.
//!
//! Bucket collisions are common and happen when two distinct hashes are assigned
//! the same bucket.
//!
//! ## Examples of linear probing
//!
//! In a perfect world, keys map uniquely to bucket. Imagine three keys assigned
//! to distinct buckets:
//! ```text
//!   Keys:   1, 2, 3
//!   Table: [1, 2, 3]
//! ```
//! To look up key `2`, we go directly to the second bucket and scan right until
//! we hit `3`.
//!
//! In a less perfect world, we may have to skip keys that were spilled from a
//! bucket further to the left in the table. Consider:
//! ```text
//!   Keys:   1, 1, 2
//!   Table: [1, 1, 2]
//! ```
//! To look up key `2`, we go to the second bucket. This bucket contains a
//! spill-over key which is skipped.
//!
//! # Code layout
//!
//! A [`CarIndex`] maps from [`cid::Cid`]s to possible [`FrameOffset`]s. The
//! mapping is unique unless the hash of two CIDs collide (possible but
//! extremely unlikely). The caller should always verify that the [`cid::Cid`] in the
//! CAR file at [`FrameOffset`] matches the requested [`cid::Cid`].
//!
//! [`CarIndexBuilder`] takes a collection of `(Cid, BlockPosition)` pairs and
//! encodes them to a writer. The only guarantees about the format is that
//! [`CarIndex`] can read it.
//!
//! ## Internal structures
//!
//! A [`Slot`] is a position in the table that may or may not be filled with a
//! [`KeyValuePair`]. [`struct@Hash`]es are key and are not required to be unique. The
//! performance of the index depends entirely on the quality of the chosen hash
//! function.
//!

mod car_index_builder;
mod hash;
mod index_header;
mod key_value_pair;
mod slot;

pub use car_index_builder::CarIndexBuilder;
use hash::Hash;
use index_header::IndexHeader;
pub use key_value_pair::FrameOffset;
use key_value_pair::KeyValuePair;
use slot::Slot;

use cid::Cid;
use smallvec::{smallvec, SmallVec};
use std::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom};

pub struct CarIndex<ReaderT> {
    pub reader: ReaderT,
    pub offset: u64,
    pub header: IndexHeader,
}

impl<ReaderT: Read + Seek> CarIndex<ReaderT> {
    /// `O(1)` Open a reader as a mapping from CIDs to frame positions in a
    /// compressed content-addressable archive.
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
    /// unless 2 or more CIDs have collided.
    pub fn lookup(&mut self, key: Cid) -> Result<SmallVec<[FrameOffset; 1]>> {
        self.lookup_internal(Hash::from(key))
    }

    #[cfg(any(test, feature = "benchmark-private"))]
    pub fn lookup_hash(&mut self, hash: Hash) -> Result<SmallVec<[FrameOffset; 1]>> {
        self.lookup_internal(hash)
    }

    // Jump to bucket offset and scan downstream. All key-value pairs with the
    // right key are guaranteed to appear before we encounter an empty slot.
    fn lookup_internal(&mut self, hash: Hash) -> Result<SmallVec<[FrameOffset; 1]>> {
        let mut limit = self.header.longest_distance;
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
            if limit == 0 {
                // Even the biggest bucket does not have this many entries. We
                // can safely return an empty result now.
                return Ok(smallvec![]);
            }
            limit -= 1;
        }
        Ok(smallvec![])
    }

    /// Gets a mutable reference to the underlying reader.
    pub fn get_mut(&mut self) -> &mut ReaderT {
        &mut self.reader
    }

    pub fn map_reader<V>(self, f: impl FnOnce(ReaderT) -> V) -> CarIndex<V> {
        CarIndex {
            reader: f(self.reader),
            offset: self.offset,
            header: self.header,
        }
    }
}

#[cfg(test)]
mod tests;
