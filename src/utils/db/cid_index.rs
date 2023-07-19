use crate::utils::cid::CidCborExt;
use cid::Cid;
use itertools::Itertools;
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::Hasher;
use std::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom, Write};
use tokio::io::{AsyncWrite, AsyncWriteExt as _};

fn hash_cid(cid: Cid) -> u64 {
    u64::from_le_bytes(cid.hash().digest()[0..8].try_into().unwrap_or([0xFF; 8]))
}

// Optimal position for a hash with a given table length
fn hash_target(hash: u64, len: usize) -> usize {
    hash as usize % len
}

// Walking distance between `at` and the optimal location of `hash`
fn hash_distance(hash: u64, at: usize, len: usize) -> usize {
    (at as isize - hash_target(hash, len) as isize).rem_euclid(len as isize) as usize
}

pub struct ProbingHashtable<ReaderT> {
    reader: ReaderT,
    // TODO(lemmih): Move offset and size into a separate seekable reader?
    offset: u64,
    len: u64, // length of table in elements. Each element is 128bit.
}

impl<ReaderT: Read + Seek> ProbingHashtable<ReaderT> {
    fn new(reader: ReaderT, offset: u64, len: u64) -> Self {
        ProbingHashtable {
            reader,
            offset,
            len,
        }
    }

    fn entries(&mut self, mut index: u64) -> Result<impl Iterator<Item = Result<Entry>> + '_> {
        if index >= self.len {
            return Err(Error::new(ErrorKind::InvalidInput, "out-of-bound index"));
        }
        let len = self.len;
        self.reader.seek(SeekFrom::Start(
            self.offset + index * std::mem::size_of::<[u8; 16]>() as u64,
        ))?;
        Ok(std::iter::from_fn(move || {
            if index == self.len {
                if let Err(err) = self.reader.seek(SeekFrom::Start(self.offset)) {
                    return Some(Err(err));
                }
                index = 0;
            }
            let mut buffer = [0; 16];
            if let Err(err) = self.reader.read_exact(&mut buffer) {
                return Some(Err(err));
            }
            index += 1;
            Some(Ok(Entry::from_le_bytes(buffer)))
        })
        .take(len as usize))
    }

    fn positions(
        &mut self,
        mut index: u64,
    ) -> Result<impl Iterator<Item = Result<(u64, Position)>> + '_> {
        Ok(self.entries(index)?.filter_map(|result| {
            result
                .map(|entry| match entry {
                    Entry::Empty => None,
                    Entry::Full { hash, value } => Some((hash, value)),
                })
                .transpose()
        }))
    }

    fn lookup(&mut self, index: u64) -> Result<impl Iterator<Item = Result<Position>> + '_> {
        let len = self.len;
        Ok(self.positions(index)?.enumerate().take_while(move |(nth, result)| {
            match result {
                Err(_) => true,
                Ok((hash, position)) => {
                    let hash_dist = hash_distance(*hash, index as usize +nth, len as usize);
                    hash_dist <= *nth
                }
            }
        }).filter_map(move |(nth, result)| {
            result.map(|(hash, position)| {
                if hash == hash_target(hash, len as usize) as u64 {
                    Some(position)
                } else {
                    None
                }
            }).transpose()
        }))
    }
}

pub struct ProbingHashtableBuilder {
    table: Vec<Entry>,
    collisions: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Empty,
    Full { hash: u64, value: Position },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            Entry::Full {
                hash,
                value: Position::decode(value),
            }
        }
    }
}

impl Position {
    // Returns None if the two offets cannot be stored in a single u64
    fn new(zst_frame_offset: u64, decoded_offset: u16) -> Option<Self> {
        let position = Position {
            zst_frame_offset,
            decoded_offset,
        };
        if position.encode() == u64::MAX || Position::decode(position.encode()) != position {
            None
        } else {
            Some(position)
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
        let len = self.table.len();
        let entry_offset = hash_target(new_key, len);
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
                    let new_distance = hash_distance(new_key, at, len);
                    let prev_distance = hash_distance(prev_key, at, len);
                    if new_distance > prev_distance {
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
                let dist = hash_distance(*hash, n, self.table.len());

                map.entry(dist).and_modify(|n| *n += 1).or_insert(1);
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
    use quickcheck::{Arbitrary, Gen};
    use quickcheck_macros::quickcheck;

    impl Arbitrary for Position {
        fn arbitrary(g: &mut Gen) -> Position {
            Position::new(
                u64::arbitrary(g).saturating_sub(1) >> u16::BITS,
                u16::arbitrary(g),
            )
            .unwrap()
        }
    }

    #[quickcheck]
    fn position_roundtrip(p: Position) {
        assert_eq!(p, Position::decode(p.encode()))
    }

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
