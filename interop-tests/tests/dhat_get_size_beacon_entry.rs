// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest::interop_tests_private::beacon::BeaconEntry;
use get_size2::GetSize;
use std::mem::size_of;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[test]
fn test_get_size_beacon_entry() {
    let _profiler = dhat::Profiler::builder().testing().build();
    let b = BeaconEntry::new(1, vec![0; 10]);

    let stats = dhat::HeapStats::get();
    dhat::assert_eq!(stats.curr_bytes, 10);
    dhat::assert_eq!(stats.curr_bytes, b.get_heap_size());

    let mut v = vec![
        b,
        BeaconEntry::new(2, vec![0; 20]),
        BeaconEntry::new(3, vec![0; 30]),
    ];

    let inner_bytes = v.iter().map(GetSize::get_heap_size).sum::<usize>();

    let stats = dhat::HeapStats::get();
    assert!(v.capacity() >= v.len());
    dhat::assert_eq!(
        stats.curr_bytes,
        size_of::<BeaconEntry>() * v.capacity() + inner_bytes
    );
    dhat::assert_eq!(stats.curr_bytes, v.get_heap_size());

    v.reserve_exact(100);

    let stats = dhat::HeapStats::get();
    assert!(v.capacity() >= 100 + v.len());
    dhat::assert_eq!(
        stats.curr_bytes,
        size_of::<BeaconEntry>() * v.capacity() + inner_bytes
    );
    dhat::assert_eq!(stats.curr_bytes, v.get_heap_size());

    v.shrink_to_fit();
    let stats = dhat::HeapStats::get();
    // `dhat::Alloc` works fine with `shrink_to_fit`
    assert_eq!(v.capacity(), v.len());
    dhat::assert_eq!(
        stats.curr_bytes,
        size_of::<BeaconEntry>() * v.capacity() + 60
    );
    dhat::assert_eq!(stats.curr_bytes, v.get_heap_size());
}
