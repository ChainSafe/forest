// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use std::io::Cursor;

use forest_filecoin::benchmark_private::{
    car_index::{CarIndex, CarIndexBuilder, FrameOffset},
    cid::CidCborExt,
};

use ahash::{HashMap, HashMapExt};
use cid::Cid;

// Benchmark lookups in car-index vs. HashMap.
// For car-index, lookups speed depends on bucket size. Bucket sizes from 0..=5
// are benchmarked, as well as max_bucket_size (worst case scenario). Average
// bucket size is ~5 for 90% load-factor and ~2 for 80% load-factor.
fn bench_car_index(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookup");

    // 2^20 =   1 million
    // 2^27 = 134 million
    let map_size: usize = 2_usize.pow(20);
    let mut map = HashMap::with_capacity(map_size);
    for i in 0..map_size as u64 {
        map.insert(Cid::from_cbor_blake2b256(&i).unwrap(), i as FrameOffset);
    }
    let live_key = Cid::from_cbor_blake2b256(&0xbeef_u64).unwrap();
    let dead_key = Cid::from_cbor_blake2b256(&"hash miss").unwrap();

    let builder = CarIndexBuilder::new((0..map_size).map(|i| {
        let i = i as u64;
        (Cid::from_cbor_blake2b256(&i).unwrap(), i as FrameOffset)
    }));

    let mut index_vec = vec![];
    builder.write(&mut index_vec).unwrap();

    let mut car_index = CarIndex::open(Cursor::new(index_vec), 0).unwrap();

    assert!(map.contains_key(&live_key));
    assert!(!map.contains_key(&dead_key));

    let _ = car_index.lookup(black_box(live_key));

    group.bench_function("hashmap/hit", |b| b.iter(|| map.get(black_box(&live_key))));

    group.bench_function("hashmap/miss", |b| b.iter(|| map.get(black_box(&dead_key))));

    for i in [0, 1, 2, 3, 4, 5, 100_u64] {
        let (hash_key, distance) = builder.hash_at_distance(i);

        group.bench_function(BenchmarkId::new("hit", distance), |b| {
            b.iter(|| car_index.lookup_hash(black_box(hash_key)))
        });
    }

    group.bench_function("miss", |b| b.iter(|| car_index.lookup(black_box(dead_key))));

    group.finish();
}

criterion_group!(benches, bench_car_index);
criterion_main!(benches);
