// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use forest_filecoin::benchmark_private::{
    cid::CidCborExt as _,
    forest::index::{self, hash, NonMaximalU64},
};
use std::{hint::black_box, num::NonZeroUsize};

// Benchmark lookups in car-index vs. HashMap.
// For car-index, lookups speed depends on bucket size. Bucket sizes from 0..=5
// are benchmarked, as well as max_bucket_size (worst case scenario). Average
// bucket size is ~5 for 90% load-factor and ~2 for 80% load-factor.
fn bench_car_index(c: &mut Criterion) {
    let live_key = Cid::from_cbor_blake2b256(&0xbeef_u64).unwrap();
    let dead_key = Cid::from_cbor_blake2b256(&"hash miss").unwrap();

    let reference = ahash::HashMap::from_iter(
        (0..1_000_000).map(|i| (Cid::from_cbor_blake2b256(&i).unwrap(), i)),
    );

    assert!(reference.contains_key(&live_key));
    assert!(!reference.contains_key(&dead_key));

    let subject_raw = index::Table::new(reference.clone(), index::DEFAULT_LOAD_FACTOR);
    let subject = {
        let mut v = vec![];
        index::Writer::new(reference.clone())
            .write_into(&mut v)
            .unwrap();
        index::Reader::new(v).unwrap()
    };

    let mut group = c.benchmark_group("lookup");

    group
        .bench_function("reference/hit", |b| {
            b.iter(|| reference.get(black_box(&live_key)))
        })
        .bench_function("reference/miss", |b| {
            b.iter(|| reference.get(black_box(&dead_key)))
        })
        .bench_function("miss", |b| b.iter(|| subject.get(black_box(dead_key))));

    for i in [0, 1, 2, 3, 4, 5, 100_u64] {
        let (hash_key, distance) = hash_at_distance(&subject_raw, i);

        group.bench_function(BenchmarkId::new("hit", distance), |b| {
            b.iter(|| subject.get_by_hash(black_box(hash_key)))
        });
    }

    group.finish();
}

criterion_group!(benches, bench_car_index);
criterion_main!(benches);

fn hash_at_distance(table: &index::Table, wanted_dist: u64) -> (NonMaximalU64, u64) {
    let mut best_diff = u64::MAX;
    let mut best_distance = u64::MAX;
    let mut best_hash = NonMaximalU64::new(0).unwrap();
    for (ix, slot) in table.slots.iter().enumerate() {
        if let index::Slot::Occupied(it) = slot {
            let ideal_ix =
                hash::ideal_slot_ix(it.hash, NonZeroUsize::new(table.initial_width).unwrap());
            let dist = (ix - ideal_ix) as u64;
            if dist.abs_diff(wanted_dist) < best_diff {
                best_diff = dist.abs_diff(wanted_dist);
                best_distance = dist;
                best_hash = it.hash;
            }
        }
    }
    (best_hash, best_distance)
}
