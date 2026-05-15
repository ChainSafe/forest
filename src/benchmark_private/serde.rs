// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Benchmarks comparing `serde_json` and `sonic-rs` on the JSON shapes that
//! flow through Forest's RPC layer.
//!
//! Run with:
//! ```console
//! $ cargo bench --features="benchmark-private" --bench serde-bench
//! ```

use crate::lotus_json::HasLotusJson;
use crate::shim::executor::Receipt;
use ::cid::Cid;
use criterion::{BenchmarkId, Criterion, Throughput};
use fvm_ipld_encoding::RawBytes;
use std::hint::black_box;

const VEC_SIZES: &[usize] = &[10, 100, 1000];

fn cid_vec(n: usize) -> Vec<Cid> {
    (0..n).map(|_| Cid::default()).collect()
}

fn sample_receipt() -> Receipt {
    Receipt::V3(fvm_shared3::receipt::Receipt {
        exit_code: fvm_shared3::error::ExitCode::new(0),
        return_data: RawBytes::new(vec![0xab; 256]),
        gas_used: 12_345,
        events_root: Some(Cid::default()),
    })
}

pub fn bench_serde(c: &mut Criterion) {
    bench_vec_cid_serialize(c);
    bench_vec_cid_deserialize(c);
    bench_lotus_vec_cid_serialize(c);
    bench_lotus_vec_cid_deserialize(c);
    bench_lotus_receipt(c);
}

/// Raw `Vec<Cid>` serialization — no LotusJson wrapper, baseline cost.
fn bench_vec_cid_serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("vec_cid/serialize");
    for &n in VEC_SIZES {
        let v = cid_vec(n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("serde_json", n), &v, |b, v| {
            b.iter(|| serde_json::to_string(black_box(v)).unwrap())
        });
        group.bench_with_input(BenchmarkId::new("sonic_rs", n), &v, |b, v| {
            b.iter(|| sonic_rs::to_string(black_box(v)).unwrap())
        });
    }
    group.finish();
}

fn bench_vec_cid_deserialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("vec_cid/deserialize");
    for &n in VEC_SIZES {
        let v = cid_vec(n);
        let json = serde_json::to_string(&v).unwrap();
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("serde_json", n), &json, |b, s| {
            b.iter(|| serde_json::from_str::<Vec<Cid>>(black_box(s)).unwrap())
        });
        group.bench_with_input(BenchmarkId::new("sonic_rs", n), &json, |b, s| {
            b.iter(|| sonic_rs::from_str::<Vec<Cid>>(black_box(s)).unwrap())
        });
    }
    group.finish();
}

/// `Vec<Cid>` through the LotusJson layer — this is the path the optimization
/// targets. Measures `into_lotus_json` + serialization together.
fn bench_lotus_vec_cid_serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("lotus_vec_cid/serialize");
    for &n in VEC_SIZES {
        let v = cid_vec(n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("serde_json", n), &v, |b, v| {
            b.iter(|| serde_json::to_string(&black_box(v.clone()).into_lotus_json()).unwrap())
        });
        group.bench_with_input(BenchmarkId::new("sonic_rs", n), &v, |b, v| {
            b.iter(|| sonic_rs::to_string(&black_box(v.clone()).into_lotus_json()).unwrap())
        });
    }
    group.finish();
}

fn bench_lotus_vec_cid_deserialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("lotus_vec_cid/deserialize");
    for &n in VEC_SIZES {
        let v = cid_vec(n);
        let json = serde_json::to_string(&v.clone().into_lotus_json()).unwrap();
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("serde_json", n), &json, |b, s| {
            b.iter(|| {
                let lj: <Vec<Cid> as HasLotusJson>::LotusJson =
                    serde_json::from_str(black_box(s)).unwrap();
                <Vec<Cid> as HasLotusJson>::from_lotus_json(lj)
            })
        });
        group.bench_with_input(BenchmarkId::new("sonic_rs", n), &json, |b, s| {
            b.iter(|| {
                let lj: <Vec<Cid> as HasLotusJson>::LotusJson =
                    sonic_rs::from_str(black_box(s)).unwrap();
                <Vec<Cid> as HasLotusJson>::from_lotus_json(lj)
            })
        });
    }
    group.finish();
}

fn bench_lotus_receipt(c: &mut Criterion) {
    let mut group = c.benchmark_group("lotus_receipt");
    let receipt = sample_receipt();
    let json = serde_json::to_string(&receipt.clone().into_lotus_json()).unwrap();

    group.bench_function("serde_json/serialize", |b| {
        b.iter(|| serde_json::to_string(&black_box(receipt.clone()).into_lotus_json()).unwrap())
    });
    group.bench_function("sonic_rs/serialize", |b| {
        b.iter(|| sonic_rs::to_string(&black_box(receipt.clone()).into_lotus_json()).unwrap())
    });
    group.bench_function("serde_json/deserialize", |b| {
        b.iter(|| {
            let lj: <Receipt as HasLotusJson>::LotusJson =
                serde_json::from_str(black_box(&json)).unwrap();
            <Receipt as HasLotusJson>::from_lotus_json(lj)
        })
    });
    group.bench_function("sonic_rs/deserialize", |b| {
        b.iter(|| {
            let lj: <Receipt as HasLotusJson>::LotusJson =
                sonic_rs::from_str(black_box(&json)).unwrap();
            <Receipt as HasLotusJson>::from_lotus_json(lj)
        })
    });
    group.finish();
}
