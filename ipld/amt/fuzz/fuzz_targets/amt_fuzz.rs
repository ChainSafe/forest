#![no_main]
use arbitrary::Arbitrary;
use ipld_amt::{Amt, MAX_INDEX};
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
struct Operation {
    idx: u64,
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
    let mut amt = Amt::new(&db);
    let mut elements = ahash::AHashMap::new();

    let flush_rate = (flush_rate as usize).saturating_add(5);
    for (i, Operation { idx, method }) in operations.into_iter().enumerate() {
        if i % flush_rate == 0 {
            // Periodic flushing of Amt to fuzz blockstore usage also
            amt.flush().unwrap();
        }
        if idx > MAX_INDEX {
            continue;
        }

        match method {
            Method::Insert(v) => {
                elements.insert(idx, v);
                amt.set(idx, v).unwrap();
            }
            Method::Remove => {
                let el = elements.remove(&idx);
                let amt_deleted = amt.delete(idx).unwrap();
                assert_eq!(amt_deleted, el.is_some());
            }
            Method::Get => {
                let ev = elements.get(&idx);
                let av = amt.get(idx).unwrap();
                assert_eq!(av, ev);
            }
        }
    }
});
