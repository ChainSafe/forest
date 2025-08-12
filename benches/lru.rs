// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! ```console
//! $ cargo bench --bench lru
//! ```

use criterion::{Criterion, criterion_group, criterion_main};
use rand::{SeedableRng as _, seq::SliceRandom as _};
use std::hint::black_box;

const LRU_CAPACITY: usize = 131072;

fn bench_lru_caches(c: &mut Criterion) {
    let input = gen_input();
    let mut group: criterion::BenchmarkGroup<'_, criterion::measurement::WallTime> =
        c.benchmark_group("LRU");
    group
        .bench_function("lru::LruCache::push", |b| {
            b.iter(|| {
                let mut c = lru::LruCache::new(LRU_CAPACITY.try_into().unwrap());
                for (k, v) in black_box(&input).iter() {
                    c.push(k, v);
                }
            })
        })
        .bench_function("lru::LruCache::get", |b| {
            let mut c = lru::LruCache::new(LRU_CAPACITY.try_into().unwrap());
            for i in 0..LRU_CAPACITY {
                c.push(i, format!("{i}"));
            }
            b.iter(|| {
                for (k, _) in black_box(&input).iter() {
                    black_box(&mut c).get(k);
                }
            })
        })
        .bench_function("hashlink::LruCache::insert", |b| {
            b.iter(|| {
                let mut c = hashlink::LruCache::new(LRU_CAPACITY);
                for (k, v) in black_box(&input).iter() {
                    c.insert(k, v);
                }
            })
        })
        .bench_function("hashlink::LruCache::get", |b| {
            let mut c = hashlink::LruCache::new(LRU_CAPACITY);
            for i in 0..LRU_CAPACITY {
                c.insert(i, format!("{i}"));
            }
            b.iter(|| {
                for (k, _) in black_box(&input).iter() {
                    black_box(&mut c).get(k);
                }
            })
        });
    group.finish();
}

fn gen_input() -> Vec<(usize, String)> {
    let mut v = Vec::with_capacity(LRU_CAPACITY * 2);
    for i in 0..LRU_CAPACITY {
        v.push((i, format!("{i}")));
        v.push((i, format!("{i}")));
    }
    let mut rng = rand_chacha::ChaChaRng::seed_from_u64(1024);
    v.shuffle(&mut rng);
    v
}

criterion_group!(benches, bench_lru_caches);
criterion_main!(benches);
