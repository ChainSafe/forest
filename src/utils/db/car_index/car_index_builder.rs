// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::{FrameOffset, Hash, IndexHeader, KeyValuePair, Slot};
use tokio::io::{AsyncWrite, AsyncWriteExt as _};

#[derive(Debug)]
pub struct CarIndexBuilder {
    table: Vec<Slot>,
    pub longest_distance: u64,
    pub collisions: u64,
    capacity: usize,
}

impl CarIndexBuilder {
    // Number of buckets given `len` number of elements
    pub fn capacity_at(len: usize) -> usize {
        // The load-factor determines the average number of bucket a lookup has
        // to scan. The formula, with 'a' being the load factor, is:
        // (1+1/(1-a))/2 A load-factor of 0.8 means lookup has to scan through 3
        // slots on average. A load-factor of 0.9 means we have to scan through
        // 5.5 slots on average. See the car_index benchmark for measurements of
        // scans at different lengths.
        let load_factor = 0.8_f64;
        (len as f64 / load_factor) as usize
    }

    // Construct a new index builder that maps `Cid` to `FrameOffset`.
    pub fn new(values: impl ExactSizeIterator<Item = (Hash, FrameOffset)>) -> CarIndexBuilder {
        let size = Self::capacity_at(values.len());
        let mut vec = Vec::with_capacity(size);
        vec.resize(size, Slot::Empty);
        let mut table = CarIndexBuilder {
            table: vec,
            collisions: 0,
            longest_distance: 0,
            capacity: values.len(),
        };
        for (hash, value) in values {
            table.insert(KeyValuePair { hash, value })
        }
        table
    }

    #[cfg(feature = "benchmark-private")]
    pub fn hash_at_distance(&self, wanted_dist: u64) -> (Hash, u64) {
        let mut best_diff = u64::MAX;
        let mut best_distance = u64::MAX;
        let mut best_hash = Hash::from(0_u64);
        for (nth, slot) in self.table.iter().enumerate() {
            if let Slot::Full(entry) = slot {
                let dist = entry.hash.distance(nth as u64, self.len());
                if dist > self.len() {
                    continue;
                }
                if dist.abs_diff(wanted_dist) < best_diff {
                    best_diff = dist.abs_diff(wanted_dist);
                    best_distance = dist;
                    best_hash = entry.hash;
                }
            }
        }
        (best_hash, best_distance)
    }

    fn insert(&mut self, mut new: KeyValuePair) {
        if self.capacity == 0 {
            panic!("cannot insert values into a full table");
        }
        self.capacity -= 1;

        let len = self.table.len() as u64;
        let mut at = new.bucket(len);
        loop {
            match self.table[at as usize] {
                Slot::Empty => {
                    self.longest_distance = self.longest_distance.max(new.distance(at, len));
                    self.table[at as usize] = Slot::Full(new);
                    break;
                }
                Slot::Full(found) => {
                    if found.hash == new.hash {
                        self.collisions += 1;
                    }
                    let found_dist = found.distance(at, len);
                    let new_dist = new.distance(at, len);
                    self.longest_distance = self.longest_distance.max(new_dist);

                    if found_dist < new_dist || (found_dist == new_dist && new.hash < found.hash) {
                        self.table[at as usize] = Slot::Full(new);
                        new = found;
                    }
                    at = (at + 1) % len;
                }
            }
        }
    }

    fn header(&self) -> IndexHeader {
        IndexHeader {
            magic_number: IndexHeader::MAGIC_NUMBER,
            longest_distance: self.longest_distance,
            collisions: self.collisions,
            buckets: self.len(),
        }
    }

    #[cfg(any(test, feature = "benchmark-private"))]
    pub fn write(&self, mut writer: impl std::io::Write) -> std::io::Result<()> {
        writer.write_all(&self.header().to_le_bytes())?;
        for slot in self.table.iter() {
            writer.write_all(&slot.to_le_bytes())?;
        }
        for i in 0..self.longest_distance {
            writer.write_all(&self.table[i as usize].to_le_bytes())?;
        }
        writer.write_all(&Slot::Empty.to_le_bytes())?;
        Ok(())
    }

    pub async fn write_async(&self, writer: &mut (impl AsyncWrite + Unpin)) -> std::io::Result<()> {
        writer.write_all(&self.header().to_le_bytes()).await?;
        for entry in self.table.iter() {
            writer.write_all(&entry.to_le_bytes()).await?;
        }
        for i in 0..self.longest_distance {
            writer
                .write_all(&self.table[i as usize].to_le_bytes())
                .await?;
        }
        writer.write_all(&Slot::Empty.to_le_bytes()).await?;
        Ok(())
    }

    pub fn encoded_len(&self) -> u32 {
        let mut len = 0;
        len += IndexHeader::SIZE;
        len += Slot::SIZE * (self.table.len() + self.longest_distance as usize + 1);
        len as u32
    }

    pub fn len(&self) -> u64 {
        self.table.len() as u64
    }
}
