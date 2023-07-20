#![allow(dead_code)]
use cid::Cid;
use std::collections::BTreeMap;
use std::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom, Write};
use std::ops::Not;
use tokio::io::{AsyncWrite, AsyncWriteExt as _};

pub struct CarIndex<ReaderT> {
    reader: ReaderT,
    offset: u64,
    len: u64, // length of table in elements. Each element is 128bit.
}

impl<ReaderT: Read + Seek> CarIndex<ReaderT> {
    fn new(reader: ReaderT, offset: u64, len: u64) -> Self {
        CarIndex {
            reader,
            offset,
            len,
        }
    }

    fn slots(&mut self, mut index: u64) -> Result<impl Iterator<Item = Result<Slot>> + '_> {
        if index >= self.len {
            return Err(Error::new(ErrorKind::InvalidInput, "out-of-bound index"));
        }
        let len = self.len;
        self.reader
            .seek(SeekFrom::Start(self.offset + index * Slot::SIZE as u64))?;
        Ok(std::iter::from_fn(move || {
            if index == self.len {
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

    fn entries(&mut self, index: u64) -> Result<impl Iterator<Item = Result<KeyValuePair>> + '_> {
        Ok(self.slots(index)?.filter_map(|result| {
            result
                .map(|entry| match entry {
                    Slot::Empty => None,
                    Slot::Full(entry) => Some(entry),
                })
                .transpose()
        }))
    }

    fn lookup(&mut self, hash: Hash) -> Result<impl Iterator<Item = Result<Position>> + '_> {
        let len = self.len;
        let key = hash.optimal_position(len as usize) as u64;
        self.reader
            .seek(SeekFrom::Start(self.offset + key * Slot::SIZE as u64))?;
        Ok(match Slot::read(&mut self.reader)? {
            Slot::Empty => itertools::Either::Left(std::iter::empty()),
            Slot::Full(first_entry) => {
                let mut smallest_dist = first_entry.hash.distance(key as usize, len as usize);
                itertools::Either::Right(
                    self.entries(key)?
                        .take_while(move |result| match result {
                            Err(_) => true,
                            Ok(entry) => {
                                let hash_dist = entry.hash.distance(key as usize, len as usize);
                                smallest_dist = smallest_dist.min(hash_dist);
                                hash_dist == smallest_dist
                            }
                        })
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
                        }),
                )
            }
        })
    }
}

#[derive(Debug)]
pub struct CarIndexBuilder {
    table: Vec<Slot>,
    collisions: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Empty,
    Full(KeyValuePair),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KeyValuePair {
    hash: Hash,
    value: Position,
}

impl KeyValuePair {
    // Optimal position for a hash with a given table length
    fn optimal_position(&self, len: usize) -> usize {
        self.hash.optimal_position(len)
    }

    // Walking distance between `at` and the optimal location of `hash`
    fn distance(&self, at: usize, len: usize) -> usize {
        self.hash.distance(at, len)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hash(u64);

impl Not for Hash {
    type Output = Hash;
    fn not(self) -> Hash {
        Hash(self.0.not())
    }
}

impl From<Hash> for u64 {
    fn from(Hash(hash): Hash) -> u64 {
        hash
    }
}

impl From<u64> for Hash {
    fn from(hash: u64) -> Hash {
        Hash(hash)
    }
}

impl From<Cid> for Hash {
    fn from(cid: Cid) -> Hash {
        Hash::from_le_bytes(cid.hash().digest()[0..8].try_into().unwrap_or([0xFF; 8]))
    }
}

impl Hash {
    const MAX: Hash = Hash(u64::MAX);

    fn from_le_bytes(bytes: [u8; 8]) -> Hash {
        Hash(u64::from_le_bytes(bytes))
    }

    // Optimal position for a hash with a given table length
    fn optimal_position(&self, len: usize) -> usize {
        self.0 as usize % len
    }

    // Walking distance between `at` and the optimal location of `hash`
    fn distance(&self, at: usize, len: usize) -> usize {
        let pos = self.optimal_position(len);
        if pos > at {
            (len - pos + at) % len
        } else {
            (at - pos) % len
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Position {
    zst_frame_offset: u64,
    decoded_offset: u16,
}

impl Slot {
    const SIZE: usize = 16;

    fn to_le_bytes(self) -> [u8; Self::SIZE] {
        let (key, value) = match self {
            Slot::Empty => (u64::MAX, u64::MAX),
            Slot::Full(entry) => (entry.hash.into(), entry.value.encode()),
        };
        let mut output: [u8; 16] = [0; 16];
        output[0..8].copy_from_slice(&key.to_le_bytes());
        output[8..16].copy_from_slice(&value.to_le_bytes());
        output
    }

    fn from_le_bytes(bytes: [u8; Self::SIZE]) -> Self {
        let hash = Hash::from_le_bytes(bytes[0..8].try_into().unwrap());
        let value = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        if value == u64::MAX {
            Slot::Empty
        } else {
            Slot::Full(KeyValuePair {
                hash,
                value: Position::decode(value),
            })
        }
    }

    fn read(reader: &mut impl Read) -> Result<Slot> {
        let mut buffer = [0; Self::SIZE];
        reader.read_exact(&mut buffer)?;
        Ok(Slot::from_le_bytes(buffer))
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

impl CarIndexBuilder {
    pub fn capacity_at(len: usize) -> usize {
        len * 100 / 91
    }

    pub fn new(values: &[(Cid, Position)]) -> CarIndexBuilder {
        Self::new_raw(
            &values
                .into_iter()
                .cloned()
                .map(|(cid, value)| (Hash::from(cid), value))
                .collect::<Vec<_>>(),
        )
    }

    pub fn new_raw(values: &[(Hash, Position)]) -> CarIndexBuilder {
        let size = Self::capacity_at(values.len());
        // println!(
        //     "Entries: {}, size: {}, buffer: {}",
        //     values.len(),
        //     size,
        //     size - values.len()
        // );
        let mut vec = Vec::with_capacity(size);
        vec.resize(size, Slot::Empty);
        let mut table = CarIndexBuilder {
            table: vec,
            collisions: 0,
        };
        for (hash, value) in values.into_iter().cloned() {
            table.insert(KeyValuePair { hash, value })
        }
        table
    }

    fn insert(&mut self, mut new: KeyValuePair) {
        let len = self.table.len();
        let mut at = new.optimal_position(len);
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

    fn read_misses(&self) -> BTreeMap<usize, usize> {
        let mut map = BTreeMap::new();
        for (n, elt) in self.table.iter().enumerate() {
            if let Slot::Full(KeyValuePair { hash, .. }) = elt {
                let dist = hash.distance(n, self.table.len());

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

    pub fn len(&self) -> u64 {
        self.table.len() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::cid::CidCborExt;
    use quickcheck::{Arbitrary, Gen};
    use quickcheck_macros::quickcheck;
    use std::collections::{HashMap, HashSet};
    use std::io::Cursor;

    impl Arbitrary for Position {
        fn arbitrary(g: &mut Gen) -> Position {
            Position::new(
                (u64::arbitrary(g) >> u16::BITS).saturating_sub(1),
                u16::arbitrary(g),
            )
            .unwrap()
        }
    }

    impl Arbitrary for Hash {
        fn arbitrary(g: &mut Gen) -> Hash {
            Hash::from(u64::arbitrary(g))
        }
    }

    #[quickcheck]
    fn position_roundtrip(p: Position) {
        assert_eq!(p, Position::decode(p.encode()))
    }

    // #[test]
    // fn show_misses_and_collisions() {
    //     let table = ProbingHashtableBuilder::new(
    //         &(1..=100)
    //             .map(|i| (Cid::from_cbor_blake2b256(&i).unwrap(), i))
    //             .collect::<Vec<_>>(),
    //     );
    //     dbg!(table.read_misses());
    //     dbg!(table.collisions);
    // }

    // #[test]
    // fn show_layout() {
    //     let table = ProbingHashtableBuilder::new_raw(&[
    //         (Hash(0), Position::decode(0)),
    //         (Hash(1), Position::decode(0)),
    //         (Hash(2), Position::decode(0)),
    //         (Hash(3), Position::decode(0)),
    //         (Hash(6), Position::decode(0)),
    //         (Hash(6), Position::decode(0)),
    //         (Hash(6), Position::decode(0)),
    //     ]);
    //     dbg!(table);
    // }

    fn query(table: &mut CarIndex<impl Read + Seek>, key: Hash) -> Vec<Position> {
        table
            .lookup(key)
            .unwrap()
            .collect::<Result<Vec<_>>>()
            .unwrap()
    }

    fn mk_table(entries: &[(Hash, Position)]) -> CarIndex<Cursor<Vec<u8>>> {
        let table_builder = CarIndexBuilder::new_raw(entries);
        let mut store = Vec::new();
        table_builder.write(&mut store).unwrap();
        CarIndex::new(Cursor::new(store), 0, table_builder.len())
    }

    fn mk_map(entries: &[(Hash, Position)]) -> HashMap<Hash, HashSet<Position>> {
        let mut map = HashMap::with_capacity(entries.len());
        for (hash, position) in entries.iter().copied() {
            map.entry(hash)
                .and_modify(|set: &mut HashSet<Position>| {
                    set.insert(position);
                })
                .or_insert(HashSet::from([position]));
        }
        map
    }

    #[quickcheck]
    fn lookup_singleton(key: Hash, value: Position) {
        let mut table = mk_table(&[(key, value)]);
        assert_eq!(query(&mut table, key), vec![value]);
        assert_eq!(query(&mut table, !key), vec![]);
    }

    // Identical to HashMap<Hash, HashSet<Position>> with almost no collision
    #[quickcheck]
    fn lookup_wide(entries: Vec<(Hash, Position)>) {
        let map = mk_map(&entries);
        let mut table = mk_table(&entries);
        for (&hash, value_set) in map.iter() {
            assert_eq!(&HashSet::from_iter(query(&mut table, hash)), value_set);
        }
    }

    // Identical to HashMap<Hash, HashSet<Position>> with many collision
    #[quickcheck]
    fn lookup_narrow(mut entries: Vec<(Hash, Position)>) {
        for (hash, _position) in entries.iter_mut() {
            *hash = Hash::from(u64::from(*hash) % 10);
        }
        let map = mk_map(&entries);
        let mut table = mk_table(&entries);
        for (&hash, value_set) in map.iter() {
            assert_eq!(&HashSet::from_iter(query(&mut table, hash)), value_set);
        }
    }

    // Identical to HashMap<Hash, HashSet<Position>> with few hash collisions
    // but all hash values map to optimal_position 0
    #[quickcheck]
    fn lookup_clash_all(mut entries: Vec<(Hash, Position)>) {
        let table_len = CarIndexBuilder::capacity_at(entries.len());
        for (hash, _position) in entries.iter_mut() {
            let n = u64::from(*hash);
            *hash = Hash::from(n - n % table_len as u64);
            assert_eq!(hash.optimal_position(table_len), 0);
        }
        let map = mk_map(&entries);
        let mut table = mk_table(&entries);
        for (&hash, value_set) in map.iter() {
            assert_eq!(&HashSet::from_iter(query(&mut table, hash)), value_set);
        }
    }

    // Identical to HashMap<Hash, HashSet<Position>> with few hash collisions
    // but all hash values map to optimal_position 0..10
    #[quickcheck]
    fn lookup_clash_many(mut entries: Vec<(Hash, Position)>) {
        let table_len = CarIndexBuilder::capacity_at(entries.len());
        for (hash, _position) in entries.iter_mut() {
            let n = u64::from(*hash);
            let i = n % 10.min(table_len as u64);
            *hash = Hash::from((n - n % table_len as u64).checked_add(i).unwrap_or(i));
            assert_eq!(hash.optimal_position(table_len), i as usize);
        }
        let map = mk_map(&entries);
        let mut table = mk_table(&entries);
        for (&hash, value_set) in map.iter() {
            assert_eq!(&HashSet::from_iter(query(&mut table, hash)), value_set);
        }
    }

    #[test]
    fn key_value_pair_distance_1() {
        // Hash(0) is right where it wants to be
        assert_eq!(Hash(0).distance(0, 1), 0);
    }

    #[test]
    fn key_value_pair_distance_2() {
        // If Hash(0) is at position 4 then it is 4 places away from where it wants to be.
        assert_eq!(Hash(0).distance(4, 10), 4);
    }
    #[test]
    fn key_value_pair_distance_3() {
        assert_eq!(Hash(9).distance(9, 10), 0);
        assert_eq!(Hash(9).distance(0, 10), 1);
    }
}
