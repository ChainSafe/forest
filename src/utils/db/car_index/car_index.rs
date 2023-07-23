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
    /// `O(1)` Open a reader as a mapping from cids to block positions in a
    /// content-addressable archive.
    pub fn open(reader: ReaderT, offset: u64, len: u64) -> Self {
        CarIndex {
            reader,
            offset,
            len,
        }
    }

    /// `O(1)` Look up possible `BlockPosition`s for a `Cid`. Does not allocate
    /// unless 2 or more cids have collided.
    pub fn lookup(&mut self, key: Cid) -> Result<SmallVec<[BlockPosition; 1]>> {
        self.lookup_internal(Hash::from(key))
    }

    pub fn lookup_fast(&mut self, key: Cid) -> Result<SmallVec<[BlockPosition; 1]>> {
        self.lookup_internal_fast(Hash::from(key))
    }

    // Iterate through each slot in the table starting at the nth slot.
    fn bucket_entries(&mut self, mut index: u64) -> Result<impl Iterator<Item = Result<KeyValuePair>> + '_> {
        let len = self.len;
        if index >= len {
            return Err(Error::new(ErrorKind::InvalidInput, "out-of-bound index"));
        }
        self.reader
            .seek(SeekFrom::Start(self.offset + index * Slot::SIZE as u64))?;
        Ok(std::iter::from_fn(move || {
            match Slot::read(&mut self.reader) {
                Err(e) => Some(Err(e)),
                Ok(Slot::Empty) => None,
                Ok(Slot::Full(entry)) => Some(Ok(entry))
            }
        }).take(len as usize))
    }
    // 19.5ns with take, same without

    #[cfg(any(test, feature = "benchmark-private"))]
    pub fn lookup_hash(&mut self, hash: Hash) -> Result<SmallVec<[BlockPosition; 1]>> {
        self.lookup_internal(hash)
    }

    #[cfg(any(feature = "benchmark-private"))]
    pub fn lookup_hash_fast(&mut self, hash: Hash) -> Result<SmallVec<[BlockPosition; 1]>> {
        self.lookup_internal_fast(hash)
    }

    // The entry for `hash` will always be quite close to its bucket offset. Steps:
    //  1. start iterating at the bucket offset,
    //  2. end early if we find an empty slot,
    //  3. skip any spill-over items from earlier buckets,
    //  4. take all entries in our bucket,
    //  5. filter out bucket entries that do not match our key.
    fn lookup_internal(&mut self, hash: Hash) -> Result<SmallVec<[BlockPosition; 1]>> {
        let len = self.len;
        let key = hash.bucket(len);
        let mut smallest_seen_distance = u64::MAX;

        // starting at the bucket for 'key', scan through entries, stopping at
        // empty slots.
        self.bucket_entries(key)?
            // skip entries that have spilled over from earlier buckets
            .skip_while(move |result| match result {
                Err(_) => false,
                Ok(entry) => {
                    // println!("skip_while: {:?}", entry);
                    let dist = entry.hash.distance(key, len);
                    smallest_seen_distance = smallest_seen_distance.min(dist);
                    dist == smallest_seen_distance && dist > 0
                }
            })
            // take all key-value-pairs in our bucket
            .take_while(move |result| match result {
                Err(_) => true,
                Ok(entry) => {
                    // println!("take_while: {:?}", entry);
                    entry.hash.distance(key, len) == 0
                }
            })
            // filter out key-value-pairs that do not match our key
            .filter_map(move |result| {
                result
                    .map(|entry| {
                        // println!("filter_map: {:?}", entry);
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

    fn lookup_internal_fast(&mut self, hash: Hash) -> Result<SmallVec<[BlockPosition; 1]>> {
        let len = self.len;
        let key = hash.bucket(len);
        let mut ret = smallvec![];

        self.reader
            .seek(SeekFrom::Start(self.offset + key * Slot::SIZE as u64))?;
        loop {
            let slot = Slot::read(&mut self.reader)?;
            match slot {
                Slot::Empty => return Ok(smallvec![]),
                Slot::Full(entry) => {
                    if entry.hash == hash {
                        ret.push(entry.value);
                        while let Some(value) = Slot::read_with_hash(&mut self.reader, hash)? {
                            ret.push(value);
                        }
                        return Ok(ret)
                    }
                }
            }
            // let entry_hash = read_u64(&mut self.reader)?;
            // let entry_value = read_u64(&mut self.reader)?;
            // if entry_hash == hash.0 {
            //     return Ok(smallvec![BlockPosition::decode(entry_value)]);
            //     // return Ok(smallvec![BlockPosition{zst_frame_offset: entry_value, decoded_offset: 0}]);
            //     // return Ok(smallvec![BlockPosition::default()]);
            // }
            // if entry_hash == u64::MAX {
            //     return Ok(smallvec![]);
            // }
        }
        // Ok(smallvec![])

        // let mut iter = self.slots(key)?;
        // while let Some(ret_slot) = iter.next() {
        //     let slot = ret_slot?;
        //     if let Slot::Full(entry) = slot {
        //         if entry.hash == hash {
        //             return Ok(smallvec![entry.value]);
        //         }
        //     } else {
        //         return Ok(smallvec![]);
        //     }
        // }
        // Ok(smallvec![])

        // let mut iter = self.slots(key)?;
        // loop {
        //     let ret_slot = iter.next().unwrap();
        //     let slot = ret_slot?;
        //     if let Slot::Full(entry) = slot {
        //         if entry.hash == hash {
        //             // return Ok(smallvec![entry.value]);
        //             ret.push(entry.value);
        //             loop {
        //                 let ret_slot = iter.next().unwrap();
        //                 let slot = ret_slot?;
        //                 if let Slot::Full(entry) = slot {
        //                     if entry.hash == hash {
        //                         ret.push(entry.value)
        //                     } else {
        //                         return Ok(ret)
        //                     }
        //                 } else {
        //                     return Ok(ret)
        //                 }
        //             }
        //         } else {
        //             let entry_dist = entry.hash.distance(key as usize, len as usize);
        //             if entry_dist <= smallest_seen_distance {
        //                 smallest_seen_distance = entry_dist;
        //             } else {
        //                 return Ok(ret)
        //             }
        //         }
        //     } else {
        //         return Ok(ret)
        //     }
        // }

        // for ret_slot in self.slots(key)? {
        //     let slot = ret_slot?;
        //     if let Slot::Full(entry) = slot {
        //         if entry.hash == hash {
        //             smallest_seen_distance = 0;
        //             return Ok(smallvec![entry.value]);
        //             ret.push(entry.value);
        //         } else {
        //             if smallest_seen_distance == 0 {
        //                 if entry.hash > hash {
        //                     break;
        //                 }
        //             } else {
        //                 let entry_dist = entry.hash.distance(key as usize, len as usize);
        //                 if entry_dist <= smallest_seen_distance {
        //                     smallest_seen_distance = entry_dist;
        //                 } else {
        //                     break;
        //                 }
        //             }
        //         }
        //     } else {
        //         break;
        //     }
        // }
        // Ok(ret)
        // Ok(self
        //     .entries(key)?
        //     .map(|result| result.unwrap())
        //     .map(|entry| (entry, entry.hash.distance(key as usize, len as usize)))
        //     // skip entries that have spilled over from earlier buckets
        //     .skip_while(|(entry, entry_dist)| {
        //         // println!("skip_while: {:?}", entry);
        //         smallest_seen_distance = smallest_seen_distance.min(*entry_dist);
        //         *entry_dist == smallest_seen_distance && *entry_dist > 0
        //     })
        //     // take all key-value-pairs in our bucket
        //     .take_while(move |(entry, entry_dist)| {
        //         // println!("take_while: {:?}", entry);
        //         *entry_dist == 0
        //     })
        //     // filter out key-value-pairs that do not match our key
        //     .filter_map(move |(entry, _entry_dist)| {
        //         // println!("filter_map: {:?}", entry);
        //         if hash == entry.hash {
        //             Some(entry.value)
        //         } else {
        //             None
        //         }
        //     })
        //     .collect::<SmallVec<_>>())
    }
}

pub fn read_u64(reader: &mut impl Read) -> Result<u64> {
    let mut buffer = [0; 8];
    reader.read_exact(&mut buffer)?;
    Ok(u64::from_le_bytes(buffer))
}
