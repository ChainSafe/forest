// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

fn fibonacci_slow(n: u64) -> u64 {
    match n {
        0 => 1,
        1 => 1,
        n => fibonacci_slow(n - 1) + fibonacci_slow(n - 2),
    }
}

fn fibonacci_fast(n: u64) -> u64 {
    let mut a = 0;
    let mut b = 1;

    match n {
        0 => b,
        _ => {
            for _ in 0..n {
                let c = a + b;
                a = b;
                b = c;
            }
            b
        }
    }
}

fn bench_fibs(c: &mut Criterion) {
    let mut group = c.benchmark_group("Fibonacci");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for i in 0..5 {
        let i = &i;
        group.bench_with_input(BenchmarkId::new("Recursive", i), i, |b, i| {
            b.iter(|| fibonacci_slow(*i))
        });
        group.bench_with_input(BenchmarkId::new("Iterative", i), i, |b, i| {
            b.iter(|| fibonacci_fast(*i))
        });
        group.bench_with_input(BenchmarkId::new("Recursive Async", i), i, |b, i| {
            b.to_async(&runtime).iter(|| async { fibonacci_slow(*i) })
        });
        group.bench_with_input(BenchmarkId::new("Iterative Async", i), i, |b, i| {
            b.to_async(&runtime).iter(|| async { fibonacci_fast(*i) })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_fibs);
criterion_main!(benches);
