// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[test]
fn test_lru() {
    let _profiler = dhat::Profiler::builder().testing().build();

    let mut c = lru::LruCache::new(10000.try_into().unwrap());
    for i in 0..10 {
        c.push(i, format!("i"));
    }

    let stats = dhat::HeapStats::get();
    assert!(
        stats.curr_bytes >= 278000,
        "curr_bytes: {}",
        stats.curr_bytes
    );
}
