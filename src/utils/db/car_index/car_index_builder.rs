// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(dead_code)]
use super::{BlockPosition, Hash, KeyValuePair, Slot, IndexHeader};
use cid::Cid;
use std::collections::BTreeMap;
use std::io::Write;
use tokio::io::{AsyncWrite, AsyncWriteExt as _};

#[derive(Debug)]
pub struct CarIndexBuilder {
    table: Vec<Slot>,
    pub longest_distance: u64,
    pub collisions: u64,
    capacity: usize,
}

impl CarIndexBuilder {
    pub fn capacity_at(len: usize) -> usize {
        len * 100 / 81
    }

    pub fn new(values: impl ExactSizeIterator<Item = (Cid, BlockPosition)>) -> CarIndexBuilder {
        Self::new_raw(values.map(|(cid, value)| (Hash::from(cid), value)))
    }

    pub fn new_raw(
        values: impl ExactSizeIterator<Item = (Hash, BlockPosition)>,
    ) -> CarIndexBuilder {
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

    pub fn avg_distance(&self) -> f64 {
        let mut distances = vec![];
        for (nth, slot) in self.table.iter().enumerate() {
            if let Slot::Full(entry) = slot {
                let dist = entry.hash.distance(nth as u64, self.len());
                distances.push(dist as f64);
            }
        }
        distances.iter().sum::<f64>() / distances.len() as f64
    }

    pub fn avg_steps_to_empty(&self) -> f64 {
        let mut distances = vec![];
        for nth in 0..self.table.len() {
            let mut steps = 0;
            let mut iter = self.table.iter().cycle().skip(nth);
            while let Some(Slot::Full(_)) = iter.next() {
                steps += 1;
            }
            distances.push(steps as f64);
        }
        distances.iter().sum::<f64>() / distances.len() as f64
    }

    fn insert(&mut self, mut new: KeyValuePair) {
        if self.capacity == 0 {
            panic!("cannot insert values into a full table");
        }
        self.capacity -= 1;

        let len = self.table.len() as u64;
        let mut at = new.optimal_offset(len);
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

    pub fn read_misses(&self) -> BTreeMap<u64, u64> {
        let mut map = BTreeMap::new();
        for (n, elt) in self.table.iter().enumerate() {
            if let Slot::Full(KeyValuePair { hash, .. }) = elt {
                let dist = hash.distance(n as u64, self.table.len() as u64);

                map.entry(dist).and_modify(|n| *n += 1).or_insert(1);
            }
        }
        map
    }

    fn header(&self) -> IndexHeader {
        IndexHeader {
            magic_number: 0xdeadbeef,
            longest_distance: self.longest_distance,
            collisions: self.collisions,
            buckets: self.len(),
        }
    }

    pub fn write(&self, mut writer: impl Write) -> std::io::Result<()> {
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

    pub async fn write_async(&self, mut writer: impl AsyncWrite + Unpin) -> std::io::Result<()> {
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

    pub fn len(&self) -> u64 {
        self.table.len() as u64
    }
}
