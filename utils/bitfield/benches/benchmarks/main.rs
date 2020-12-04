// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod examples;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use forest_bitfield::BitField;

use examples::{example1, example2};

fn len(c: &mut Criterion) {
    let bf = example1();
    c.bench_function("len", |b| b.iter(|| black_box(&bf).len()));
}

fn bits(c: &mut Criterion) {
    let bf = example1();
    c.bench_function("bits", |b| {
        b.iter(|| black_box(&bf).iter().map(black_box).count())
    });
}

fn new(c: &mut Criterion) {
    c.bench_function("new", |b| b.iter(|| example1()));
}

fn decode_encode(c: &mut Criterion) {
    let bf = example1();
    c.bench_function("decode_encode", |b| {
        b.iter(|| BitField::from_ranges(bf.ranges()))
    });
}

fn from_ranges(c: &mut Criterion) {
    let vec: Vec<_> = example1().ranges().collect();
    let ranges = || forest_bitfield::iter::Ranges::new(vec.iter().cloned());
    c.bench_function("from_ranges", |b| {
        b.iter(|| BitField::from_ranges(ranges()))
    });
}

fn is_empty(c: &mut Criterion) {
    let bf = example1();
    c.bench_function("is_empty", |b| b.iter(|| bf.is_empty()));
}

fn intersection(c: &mut Criterion) {
    let bf1 = example1();
    let bf2 = example2();
    c.bench_function("intersection", |b| b.iter(|| &bf1 & &bf2));
}

fn union(c: &mut Criterion) {
    let bf1 = example1();
    let bf2 = example2();
    c.bench_function("union", |b| b.iter(|| &bf1 | &bf2));
}

fn difference(c: &mut Criterion) {
    let bf1 = example1();
    let bf2 = example2();
    c.bench_function("difference", |b| b.iter(|| &bf1 - &bf2));
}

fn symmetric_difference(c: &mut Criterion) {
    let bf1 = example1();
    let bf2 = example2();
    c.bench_function("symmetric_difference", |b| b.iter(|| &bf1 ^ &bf2));
}

fn cut(c: &mut Criterion) {
    let bf1 = example1();
    let bf2 = example2();
    c.bench_function("cut", |b| b.iter(|| bf1.cut(&bf2)));
}

fn contains_all(c: &mut Criterion) {
    let bf1 = example1();
    let bf2 = example2();
    let intersection = &bf1 & &bf2;
    c.bench_function("contains_all", |b| {
        b.iter(|| bf1.contains_all(&intersection))
    });
}

fn contains_any(c: &mut Criterion) {
    let bf1 = example1();
    let bf2 = example2();
    let difference = &bf1 - &bf2;
    c.bench_function("contains_any", |b| b.iter(|| bf2.contains_any(&difference)));
}

fn get(c: &mut Criterion) {
    let bf = example1();
    c.bench_function("get", |b| b.iter(|| bf.get(500_000)));
}

criterion_group!(
    benches,
    len,
    bits,
    new,
    decode_encode,
    from_ranges,
    is_empty,
    intersection,
    union,
    difference,
    symmetric_difference,
    cut,
    contains_all,
    contains_any,
    get,
);
criterion_main!(benches);
