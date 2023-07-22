// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use std::io::Cursor;

use forest_filecoin::benchmark_private::{car_index::{CarIndexBuilder,BlockPosition, CarIndex}, cid::CidCborExt};

use ahash::{HashMap, HashMapExt};
use cid::Cid;

fn bench_fibs(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookup");
    
    let map_size:usize = 2_usize.pow(20);
    let mut map = HashMap::with_capacity(map_size);
    for i in 0..map_size as u64 {
        map.insert(Cid::from_cbor_blake2b256(&i).unwrap(), BlockPosition::new(i, 0).unwrap());
    }
    let live_key = Cid::from_cbor_blake2b256(&0xbeef_u64).unwrap();
    let dead_key = Cid::from_cbor_blake2b256(&"hash miss").unwrap();

    let builder = CarIndexBuilder::new(&map.clone().into_iter().collect::<Vec<_>>());
    let mut index_vec = vec![];
    builder.write(&mut index_vec).unwrap();
    let mut car_index = CarIndex::open(Cursor::new(index_vec), 0, builder.len());
    
    assert!(map.contains_key(&live_key));
    assert!(!map.contains_key(&dead_key));

    group.bench_function("hashmap/hit", |b| {
        b.iter(|| map.get(black_box(&live_key)))
    });

    group.bench_function("hashmap/miss", |b| {
        b.iter(|| map.get(black_box(&dead_key)))
    });

    group.bench_function("car/hit", |b| {
        b.iter(|| car_index.lookup(black_box(live_key)))
    });

    group.bench_function("car/miss", |b| {
        b.iter(|| car_index.lookup(black_box(dead_key)))
    });

    group.finish();
}

criterion_group!(benches, bench_fibs);
criterion_main!(benches);
