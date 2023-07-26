// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use ahash::{AHashMap, AHashSet};
use quickcheck_macros::quickcheck;
use std::io::{Cursor, Read, Seek};

fn query(table: &mut CarIndex<impl Read + Seek>, key: Hash) -> Vec<BlockPosition> {
    table.lookup_hash(key).unwrap().into_vec()
}

fn mk_table(entries: &[(Hash, BlockPosition)]) -> CarIndex<Cursor<Vec<u8>>> {
    let table_builder = CarIndexBuilder::new_raw(entries.iter().copied());
    let mut store = Vec::new();
    table_builder.write(&mut store).unwrap();
    CarIndex::open(Cursor::new(store), 0).unwrap()
}

fn mk_map(entries: &[(Hash, BlockPosition)]) -> AHashMap<Hash, AHashSet<BlockPosition>> {
    let mut map = AHashMap::with_capacity(entries.len());
    for (hash, position) in entries.iter().copied() {
        map.entry(hash)
            .and_modify(|set: &mut AHashSet<BlockPosition>| {
                set.insert(position);
            })
            .or_insert(AHashSet::from([position]));
    }
    map
}

#[quickcheck]
fn lookup_singleton(key: Hash, value: BlockPosition) {
    let mut table = mk_table(&[(key, value)]);
    assert_eq!(query(&mut table, key), vec![value]);
    assert_eq!(query(&mut table, !key), Vec::<BlockPosition>::new());
}

// Identical to HashMap<Hash, HashSet<Position>> with almost no collision
#[quickcheck]
fn lookup_wide(entries: Vec<(Hash, BlockPosition)>) {
    let map = mk_map(&entries);
    let mut table = mk_table(&entries);
    for (&hash, value_set) in map.iter() {
        assert_eq!(&AHashSet::from_iter(query(&mut table, hash)), value_set);
    }
}

// Identical to HashMap<Hash, HashSet<Position>> with many collision
#[quickcheck]
fn lookup_narrow(mut entries: Vec<(Hash, BlockPosition)>) {
    for (hash, _position) in entries.iter_mut() {
        *hash = Hash::from(u64::from(*hash) % 10);
    }
    let map = mk_map(&entries);
    let mut table = mk_table(&entries);
    for (&hash, value_set) in map.iter() {
        assert_eq!(&AHashSet::from_iter(query(&mut table, hash)), value_set);
    }
}

// Identical to HashMap<Hash, HashSet<Position>> with few hash collisions
// but all hash values map to optimal_position 0
#[quickcheck]
fn lookup_clash_all(mut entries: Vec<(Hash, BlockPosition)>) {
    let table_len = CarIndexBuilder::capacity_at(entries.len()) as u64;
    for (hash, _position) in entries.iter_mut() {
        *hash = hash.set_bucket(0, table_len);
        assert_eq!(hash.bucket(table_len), 0);
    }
    let map = mk_map(&entries);
    let mut table = mk_table(&entries);
    for (&hash, value_set) in map.iter() {
        assert_eq!(&AHashSet::from_iter(query(&mut table, hash)), value_set);
    }
}

// Identical to HashMap<Hash, HashSet<Position>> with few hash collisions
// but all hash values map to optimal_position 0..10
#[quickcheck]
fn lookup_clash_many(mut entries: Vec<(Hash, BlockPosition)>) {
    let table_len = CarIndexBuilder::capacity_at(entries.len()) as u64;
    for (hash, _position) in entries.iter_mut() {
        let i = u64::from(*hash) % 10.min(table_len);
        *hash = hash.set_bucket(i, table_len);
        assert_eq!(hash.bucket(table_len), i);
    }
    let map = mk_map(&entries);
    let mut table = mk_table(&entries);
    for (&hash, value_set) in map.iter() {
        assert_eq!(&AHashSet::from_iter(query(&mut table, hash)), value_set);
    }
}
