// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::str::FromStr as _;

use super::*;
use crate::{
    db::car::{forest::tests::mk_encoded_car, ForestCar},
    utils::{cid::CidCborExt as _, db::car_stream::CarBlock},
};
use ahash::{AHashMap, AHashSet};
use itertools::Itertools;
use pretty_assertions::assert_eq;
use quickcheck::Arbitrary;
use quickcheck_macros::quickcheck;

fn query(table: &CarIndex<impl ReadAt>, key: Hash) -> Vec<FrameOffset> {
    table.lookup_hash(key).unwrap().into_vec()
}

fn mk_table(entries: &[(Hash, FrameOffset)]) -> CarIndex<Vec<u8>> {
    let table_builder = CarIndexBuilder::new(entries.iter().copied());
    let mut store = Vec::new();
    table_builder.write(&mut store).unwrap();
    dbg!(&store[32..32 + 8]);
    CarIndex::open(store, 0).unwrap()
}

fn mk_map(entries: &[(Hash, FrameOffset)]) -> AHashMap<Hash, AHashSet<FrameOffset>> {
    let mut map = AHashMap::with_capacity(entries.len());
    for (hash, position) in entries.iter().copied() {
        map.entry(hash)
            .and_modify(|set: &mut AHashSet<FrameOffset>| {
                set.insert(position);
            })
            .or_insert(AHashSet::from([position]));
    }
    map
}

#[quickcheck]
fn lookup_singleton(key: Hash, value: FrameOffset) {
    let table = mk_table(&[(key, value)]);
    assert_eq!(query(&table, key), vec![value]);
    assert_eq!(query(&table, !key), Vec::<FrameOffset>::new());
}

// Identical to HashMap<Hash, HashSet<FrameOffset>> with almost no collision
#[quickcheck]
fn lookup_wide(entries: Vec<(Hash, FrameOffset)>) {
    let map = mk_map(&entries);
    let table = mk_table(&entries);
    for (&hash, value_set) in map.iter() {
        assert_eq!(&AHashSet::from_iter(query(&table, hash)), value_set);
    }
}

// Identical to HashMap<Hash, HashSet<FrameOffset>> with many collision
#[quickcheck]
fn lookup_narrow(mut entries: Vec<(Hash, FrameOffset)>) {
    for (hash, _position) in entries.iter_mut() {
        *hash = Hash::from(u64::from(*hash) % 10);
    }
    let map = mk_map(&entries);
    let table = mk_table(&entries);
    for (&hash, value_set) in map.iter() {
        assert_eq!(&AHashSet::from_iter(query(&table, hash)), value_set);
    }
}

// Identical to HashMap<Hash, HashSet<FrameOffset>> with few hash collisions
// but all hash values map to optimal_position 0
#[quickcheck]
fn lookup_clash_all(mut entries: Vec<(Hash, FrameOffset)>) {
    let table_len = CarIndexBuilder::capacity_at(entries.len()) as u64;
    for (hash, _position) in entries.iter_mut() {
        *hash = hash.set_bucket(0, table_len);
        assert_eq!(hash.bucket(table_len), 0);
    }
    let map = mk_map(&entries);
    let table = mk_table(&entries);
    for (&hash, value_set) in map.iter() {
        assert_eq!(&AHashSet::from_iter(query(&table, hash)), value_set);
    }
}

// Identical to HashMap<Hash, HashSet<FrameOffset>> with few hash collisions
// but all hash values map to optimal_position 0..10
#[quickcheck]
fn lookup_clash_many(mut entries: Vec<(Hash, FrameOffset)>) {
    let table_len = CarIndexBuilder::capacity_at(entries.len()) as u64;
    for (hash, _position) in entries.iter_mut() {
        let i = u64::from(*hash) % 10.min(table_len);
        *hash = hash.set_bucket(i, table_len);
        assert_eq!(hash.bucket(table_len), i);
    }
    let map = mk_map(&entries);
    let table = mk_table(&entries);
    for (hash, _) in entries.into_iter() {
        assert_eq!(&AHashSet::from_iter(query(&table, hash)), &map[&hash]);
    }
}

#[quickcheck]
fn doit(blocks: Vec<CarBlock>) {
    println!("start test");
    for CarBlock { cid, .. } in &blocks {
        println!("\t{}", cid);
    }
    let expected = blocks
        .iter()
        .map(|it| Hash::from(it.cid))
        .unique()
        .sorted()
        .collect::<Vec<_>>();
    let car = mk_encoded_car(1024 * 4, 3, vec![], blocks);
    let actual = ForestCar::new(car)
        .unwrap()
        .index()
        .iter()
        .map(Result::unwrap)
        .map(|it| it.hash)
        .sorted()
        .collect::<Vec<_>>();

    assert_eq!(expected, actual);
    println!("end test");
}

// Not very hard to find collisions
// baeaqqbhiisf72 bafnfgblucgbmnly
// bafyh4azejvfa baffukavz24
// bafhr2aji baf6c4azbblaa
// baf3goa2ssmcq bae3smb7prd3s4jpmti
// baelxsbulpqaenwvb bafjt2bh3nnfvy
// bafhxsbclkdkas bafdhabuo2btuxodj
// bafuw6byggkvhwd2lre bafcugazsw37q
// bafodsazgxsxa baf2bcbacasl62
// baeiukay3spca baejuoauxwu
// baelqiaykjisa bae4cwb6p4kzooj6dni
// bafwgcaor bae7tealp
// bafigybrzposdopog baexrgame
// bafevia5amyoa bafvhoal3
// baezhub75hxwnooki3i baetw6aa
// bafeeob5e3etqs6mbny bafsgwaa
#[test]
#[ignore]
fn find_collision() {
    let _ = quickcheck::QuickCheck::new()
        .tests(1_000_000_000)
        .quicktest(Test);
    struct Test;
    impl quickcheck::Testable for Test {
        fn result(&self, g: &mut quickcheck::Gen) -> quickcheck::TestResult {
            loop {
                let left = Cid::arbitrary(g);
                let right = Cid::arbitrary(g);
                if left == right {
                    continue;
                }
                if Hash::from(left) == Hash::from(right) {
                    println!("{} {}", left, right)
                }
            }
        }
    }
}

#[test]
fn test() {
    for (name, blocks) in [
        (
            "just-default-cid.forest.car.zst",
            vec![CarBlock {
                cid: Cid::default(),
                data: vec![],
            }],
        ),
        ("empty.forest.car.zst", vec![]),
        (
            "1-2-3-4.forest.car.zst",
            Vec::from_iter((0..=3).map(|n| CarBlock {
                cid: Cid::from_cbor_blake2b256(&n).unwrap(),
                data: vec![],
            })),
        ),
        (
            "1-1.forest.car.zst",
            vec![
                CarBlock {
                    cid: Cid::from_cbor_blake2b256(&1).unwrap(),
                    data: vec![],
                };
                2
            ],
        ),
        (
            "bafwgcaor-bae7tealp.forest.car.zst",
            vec![
                CarBlock {
                    cid: Cid::from_str("bafwgcaor").unwrap(),
                    data: vec![],
                },
                CarBlock {
                    cid: Cid::from_str("bae7tealp").unwrap(),
                    data: vec![],
                },
            ],
        ),
    ] {
        let expected_hashes = blocks
            .iter()
            .map(|it| Hash::from(it.cid))
            .unique()
            .collect::<Vec<_>>();
        let bin = mk_encoded_car(1024 * 4, 3, vec![], blocks);
        let actual_hashes = ForestCar::new(bin.clone())
            .unwrap()
            .index()
            .iter()
            .map(Result::unwrap)
            .map(|it| it.hash)
            .collect::<Vec<_>>();
        println!("file: {}", name);
        println!("expected-hashes: {:?}", expected_hashes);
        println!("actual-hashes: {:?}", actual_hashes);
        println!("{}", pretty_hex::pretty_hex(&bin));
        println!();
    }
}

#[test]
fn zstd_frame_count() {
    fn count_frames(mut slice: &[u8]) -> usize {
        let mut n = 0;
        while !slice.is_empty() {
            let mut decoder = zstd::Decoder::with_buffer(slice).unwrap().single_frame();
            io::copy(&mut decoder, &mut io::sink()).unwrap();
            n += 1;
            slice = decoder.finish();
        }
        n
    }
    let empty = mk_encoded_car(10, 1, vec![], vec![]);
    assert_eq!(
        1 /* car header */ + 2, /* forest skip frames */
        count_frames(&empty)
    );
    let one = mk_encoded_car(
        10,
        1,
        vec![],
        vec![CarBlock {
            cid: Cid::default(),
            data: vec![],
        }],
    );
    assert_eq!(
        1 /* car header */ + 1 /* cid */ + 2, /* forest skip frames */
        count_frames(&one)
    );
    let two = mk_encoded_car(
        10,
        1,
        vec![],
        vec![
            CarBlock {
                cid: Cid::default(),
                data: vec![],
            },
            CarBlock {
                cid: Cid::default(),
                data: vec![],
            },
        ],
    );
    assert_eq!(
        1 /* car header */ + 1 /* cid */ + 1 /* cid */ + 2, /* forest skip frames */
        count_frames(&two)
    );
}
