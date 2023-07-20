use super::{BlockPosition, Hash, KeyValuePair, Slot};
use cid::Cid;
use std::collections::BTreeMap;
use std::io::Write;
use std::num::{NonZeroU64, NonZeroUsize};
use tokio::io::{AsyncWrite, AsyncWriteExt as _};

#[derive(Debug)]
pub struct CarIndexBuilder {
    table: Vec<Slot>,
    collisions: u64,
    capacity: usize,
}

impl CarIndexBuilder {
    pub fn capacity_at(len: usize) -> usize {
        len * 100 / 91
    }

    pub fn new(values: &[(Cid, BlockPosition)]) -> CarIndexBuilder {
        Self::new_raw(
            &values
                .into_iter()
                .cloned()
                .map(|(cid, value)| (Hash::from(cid), value))
                .collect::<Vec<_>>(),
        )
    }

    pub fn new_raw(values: &[(Hash, BlockPosition)]) -> CarIndexBuilder {
        let size = Self::capacity_at(values.len());
        let mut vec = Vec::with_capacity(size);
        vec.resize(size, Slot::Empty);
        let mut table = CarIndexBuilder {
            table: vec,
            collisions: 0,
            capacity: size,
        };
        for (hash, value) in values.into_iter().cloned() {
            table.insert(KeyValuePair { hash, value })
        }
        table
    }

    fn insert(&mut self, mut new: KeyValuePair) {
        if self.capacity == 0 {
            panic!("cannot insert values into a full table");
        }
        self.capacity -= 1;

        let len = self.table.len();
        let mut at = new.optimal_offset(len);
        loop {
            match self.table[at] {
                Slot::Empty => {
                    self.table[at] = Slot::Full(new);
                    break;
                }
                Slot::Full(found) => {
                    if found.hash == new.hash {
                        self.collisions += 1;
                    }
                    if found.distance(at, len) < new.distance(at, len) {
                        self.table[at] = Slot::Full(new);
                        new = found;
                    }
                    at = (at + 1) % self.table.len();
                }
            }
        }
    }

    pub fn read_misses(&self) -> BTreeMap<usize, usize> {
        let mut map = BTreeMap::new();
        for (n, elt) in self.table.iter().enumerate() {
            if let Slot::Full(KeyValuePair { hash, .. }) = elt {
                let dist = hash.distance(n, self.table.len());

                map.entry(dist).and_modify(|n| *n += 1).or_insert(1);
            }
        }
        map
    }

    pub fn write(&self, mut writer: impl Write) -> std::io::Result<()> {
        for entry in self.table.iter() {
            writer.write_all(&entry.to_le_bytes())?;
        }
        Ok(())
    }

    pub async fn write_async(&self, mut writer: impl AsyncWrite + Unpin) -> std::io::Result<()> {
        for entry in self.table.iter() {
            writer.write_all(&entry.to_le_bytes()).await?;
        }
        Ok(())
    }

    pub fn len(&self) -> u64 {
        self.table.len() as u64
    }
}
