// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![no_main]
use arbitrary::Arbitrary;
use ipld_hamt::Hamt;
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
struct Operation {
    key: u64,
    method: Method,
}

#[derive(Debug, Arbitrary)]
enum Method {
    Insert(u64),
    Remove,
    Get,
}

fuzz_target!(|data: (u8, Vec<Operation>)| {
    let (flush_rate, operations) = data;
    let db = db::MemoryDB::default();
    let mut hamt = Hamt::<_, _, _>::new_with_bit_width(&db, 5);
    let mut elements = ahash::AHashMap::new();

    let flush_rate = (flush_rate as usize).saturating_add(5);
    for (i, Operation { key, method }) in operations.into_iter().enumerate() {
        if i % flush_rate == 0 {
            // Periodic flushing of Hamt to fuzz blockstore usage also
            hamt.flush().unwrap();
        }

        match method {
            Method::Insert(v) => {
                elements.insert(key, v);
                hamt.set(key, v).unwrap();
            }
            Method::Remove => {
                let el = elements.remove(&key);
                let hamt_deleted = hamt.delete(&key).unwrap().map(|(_, v)| v);
                assert_eq!(hamt_deleted, el);
            }
            Method::Get => {
                let ev = elements.get(&key);
                let av = hamt.get(&key).unwrap();
                assert_eq!(av, ev);
            }
        }
    }
});
