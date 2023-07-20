// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::{BlockPosition, Hash, KeyValuePair, Slot};
use cid::Cid;
use smallvec::{smallvec, SmallVec};
use std::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom};

pub struct CarIndex<ReaderT> {
    reader: ReaderT,
    offset: u64,
    len: u64, // length of table in elements. Each element is 128bit.
}

impl<ReaderT: Read + Seek> CarIndex<ReaderT> {
    pub fn new(reader: ReaderT, offset: u64, len: u64) -> Self {
        CarIndex {
            reader,
            offset,
            len,
        }
    }

    // Look up possible `BlockPosition`s for a `Cid`. Does not allocate unless 2
    // or more cids have collided.
    pub fn lookup(&mut self, key: Cid) -> Result<SmallVec<[BlockPosition; 1]>> {
        self.lookup_internal(Hash::from(key))
    }

    // Iterate through each slot in the table starting at the nth slot.
    fn slots(&mut self, mut index: u64) -> Result<impl Iterator<Item = Result<Slot>> + '_> {
        let len = self.len;
        if index >= len {
            return Err(Error::new(ErrorKind::InvalidInput, "out-of-bound index"));
        }
        self.reader
            .seek(SeekFrom::Start(self.offset + index * Slot::SIZE as u64))?;
        Ok(std::iter::from_fn(move || {
            if index == len {
                if let Err(err) = self.reader.seek(SeekFrom::Start(self.offset)) {
                    return Some(Err(err));
                }
                index = 0;
            }
            index += 1;
            Some(Slot::read(&mut self.reader))
        })
        .take(len as usize))
    }

    // Iterate through fill key-value-pairs starting at the nth slot. Iteration
    // stops when an empty slot is found or all slots have been traversed.
    fn entries(&mut self, index: u64) -> Result<impl Iterator<Item = Result<KeyValuePair>> + '_> {
        Ok(self.slots(index)?.map_while(|result| {
            result
                .map(|entry| match entry {
                    Slot::Empty => None,
                    Slot::Full(entry) => Some(entry),
                })
                .transpose()
        }))
    }

    #[cfg(test)]
    pub fn lookup_hash(&mut self, hash: Hash) -> Result<SmallVec<[BlockPosition; 1]>> {
        self.lookup_internal(hash)
    }

    fn lookup_internal(&mut self, hash: Hash) -> Result<SmallVec<[BlockPosition; 1]>> {
        let len = self.len;
        let key = hash.optimal_offset(len as usize) as u64;
        let mut smallest_seen_distance = usize::MAX;

        // Starting at the bucket for 'key', scan through entries, stopping at
        // empty slots.
        self.entries(key)?
            // Skip entries as long as their desired distance to 'key' is
            // decreasing.
            .skip_while(move |result| match result {
                Err(_) => false,
                Ok(entry) => {
                    let dist = entry.hash.distance(key as usize, len as usize);
                    smallest_seen_distance = smallest_seen_distance.min(dist);
                    dist == smallest_seen_distance
                }
            })
            // Take all entries with a distance of 0. These are the entries in
            // the bucket we want.
            .take_while(move |result| match result {
                Err(_) => true,
                Ok(entry) => entry.hash.distance(key as usize, len as usize) == 0,
            })
            // Filter out other keys that were put in our bucket but do not
            // match our hash
            .filter_map(move |result| {
                result
                    .map(|entry| {
                        if hash == entry.hash {
                            Some(entry.value)
                        } else {
                            None
                        }
                    })
                    .transpose()
            })
            .collect::<Result<SmallVec<_>>>()
    }
}
