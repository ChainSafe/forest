use crate::utils::cid::CidCborExt;
use cid::Cid;
use itertools::Itertools;
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::Hasher;
use std::io::Write;
use tokio::io::{AsyncWrite, AsyncWriteExt as _};

fn hash_cid(cid: Cid) -> u64 {
    u64::from_le_bytes(cid.hash().digest()[0..8].try_into().unwrap_or([0xFF; 8]))
}

pub struct ProbingHashtableBuilder {
    table: Vec<Entry>,
    collisions: u64,
}

#[derive(Clone, Copy)]
enum Entry {
    Empty,
    Full { hash: u64, value: Position },
}

#[derive(Clone, Copy)]
pub struct Position {
    zst_frame_offset: u64,
    decoded_offset: u16,
}

impl Entry {
    fn to_le_bytes(self) -> [u8; 16] {
        let (key, value) = match self {
            Entry::Empty => (u64::MAX, u64::MAX),
            Entry::Full { hash, value } => (hash, value.encode()),
        };
        let mut output: [u8; 16] = [0; 16];
        output[0..8].copy_from_slice(&key.to_le_bytes());
        output[8..16].copy_from_slice(&key.to_le_bytes());
        output
    }

    fn from_le_bytes(bytes: [u8; 16]) -> Self {
        let hash = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        let value = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        if value == u64::MAX {
            Entry::Empty
        } else {
            let zst_frame_offset = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
            Entry::Full {
                hash,
                value: Position::decode(value),
            }
        }
    }
}

impl Position {
    fn new(zst_frame_offset: u64, decoded_offset: u16) -> Option<Self> {
        if zst_frame_offset >> (u64::BITS - u16::BITS) == 0 {
            Some(Position {
                zst_frame_offset,
                decoded_offset,
            })
        } else {
            None
        }
    }

    fn encode(self) -> u64 {
        assert!(self.zst_frame_offset >> (u64::BITS - u16::BITS) == 0);
        self.zst_frame_offset << u16::BITS | self.decoded_offset as u64
    }

    fn decode(value: u64) -> Self {
        Position {
            zst_frame_offset: value >> u16::BITS,
            decoded_offset: value as u16,
        }
    }
}

impl ProbingHashtableBuilder {
    pub fn new(values: &[(Cid, Position)]) -> ProbingHashtableBuilder {
        let size = values.len() * 100 / 91;
        println!(
            "Entries: {}, size: {}, buffer: {}",
            values.len(),
            size,
            size - values.len()
        );
        let mut vec = Vec::with_capacity(size);
        vec.resize(size, Entry::Empty);
        let mut table = ProbingHashtableBuilder {
            table: vec,
            collisions: 0,
        };
        for (cid, val) in values.into_iter().cloned() {
            table.insert((hash_cid(cid), val))
        }
        table
    }

    fn insert(&mut self, (mut new_key, mut new_value): (u64, Position)) {
        let entry_offset = new_key as usize % self.table.len();
        let mut at = entry_offset;
        loop {
            match self.table[at] {
                Entry::Empty => {
                    self.table[at] = Entry::Full {
                        hash: new_key,
                        value: new_value,
                    };
                    break;
                }
                Entry::Full {
                    hash: prev_key,
                    value: prev_value,
                } => {
                    if prev_key == new_key {
                        self.collisions += 1;
                    }
                    let other_offset = prev_key as usize % self.table.len();
                    if entry_offset < other_offset {
                        self.table[at] = Entry::Full {
                            hash: new_key,
                            value: new_value,
                        };
                        new_key = prev_key;
                        new_value = prev_value;
                    }
                    at = (at + 1) % self.table.len();
                }
            }
        }
    }

    fn read_misses(&self) -> BTreeMap<usize, usize> {
        let mut map = BTreeMap::new();
        for (n, elt) in self.table.iter().enumerate() {
            if let Entry::Full { hash, .. } = elt {
                let best_position = *hash as usize % self.table.len();
                let diff = (n as isize - best_position as isize)
                    .rem_euclid(self.table.len() as isize) as usize;

                map.entry(diff).and_modify(|n| *n += 1).or_insert(1);
            }
        }
        map
    }

    fn write(&self, mut writer: impl Write) -> std::io::Result<()> {
        for entry in self.table.iter() {
            writer.write_all(&entry.to_le_bytes())?;
        }
        Ok(())
    }

    async fn write_async(&self, mut writer: impl AsyncWrite + Unpin) -> std::io::Result<()> {
        for entry in self.table.iter() {
            writer.write_all(&entry.to_le_bytes()).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_insert() {
        let table = ProbingHashtableBuilder::new(
            &(1..=100_000_000)
                .map(|i| {
                    (
                        Cid::from_cbor_blake2b256(&i).unwrap(),
                        Position::new(i, 0).unwrap(),
                    )
                })
                .collect::<Vec<_>>(),
        );
        dbg!(table.read_misses());
        dbg!(table.collisions);
    }
}
