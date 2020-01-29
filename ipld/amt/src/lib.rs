// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

mod amt;
mod bitmap;
mod block_store;
mod error;
mod node;
mod root;

pub use self::amt::*;
pub use self::bitmap::*;
pub use self::block_store::*;
pub use self::error::*;
pub use self::node::*;
pub use self::root::*;

const WIDTH: usize = 8;
const MAX_INDEX: u64 = 1 << 48;

pub(crate) fn nodes_for_height(height: u32) -> u64 {
    (WIDTH as u64).pow(height)
}

// TODO move tests to folder in crate
#[cfg(test)]
mod tests {
    use super::*;
    use encoding::{ser::Serialize, to_vec};

    fn assert_get<S, DB>(a: &mut AMT<DB>, i: u64, v: &S)
    where
        S: Serialize,
        DB: BlockStore,
    {
        assert_eq!(a.get(i).unwrap().unwrap(), to_vec(&v).unwrap());
    }

    fn assert_count<DB>(a: &mut AMT<DB>, c: u64)
    where
        DB: BlockStore,
    {
        assert_eq!(a.count(), c);
    }

    #[test]
    fn constructor() {
        AMT::new(&db::MemoryDB::default());
    }

    #[test]
    fn basic_get_set() {
        let db = db::MemoryDB::default();
        let mut a = AMT::new(&db);

        a.set(2, &"foo").unwrap();
        assert_eq!(a.get(2).unwrap().unwrap(), to_vec(&"foo").unwrap());
        assert_get(&mut a, 2, &"foo");
        assert_count(&mut a, 1);
    }

    #[test]
    fn out_of_range() {
        let db = db::MemoryDB::default();
        let mut a = AMT::new(&db);

        let res = a.set(1 << 50, &"test");
        assert_eq!(res.err(), Some(Error::OutOfRange(1 << 50)));

        let res = a.set(MAX_INDEX, &"test");
        assert_eq!(res.err(), Some(Error::OutOfRange(MAX_INDEX)));

        let res = a.set(MAX_INDEX - 1, &"test");
        assert_eq!(res.err(), None);
        assert_get(&mut a, MAX_INDEX - 1, &"test");
    }

    #[test]
    fn expand() {
        let db = db::MemoryDB::default();
        let mut a = AMT::new(&db);

        a.set(2, &"foo").unwrap();
        a.set(11, &"bar").unwrap();
        a.set(79, &"baz").unwrap();

        assert_get(&mut a, 2, &"foo");
        assert_get(&mut a, 11, &"bar");
        assert_get(&mut a, 79, &"baz");

        // Flush and save root node and get cid
        let c = a.flush().unwrap();

        // Load amt with that cid
        let mut new_amt = AMT::load(&db, c).unwrap();

        assert_get(&mut new_amt, 2, &"foo");
        assert_get(&mut new_amt, 11, &"bar");
        assert_get(&mut new_amt, 79, &"baz");
    }

    #[test]
    fn bulk_insert() {
        let db = db::MemoryDB::default();
        let mut a = AMT::new(&db);

        let iterations: u64 = 5000;

        for i in 0..iterations {
            a.set(i, &"foo foo bar").unwrap();
        }

        for i in 0..iterations {
            assert_get(&mut a, i, &"foo foo bar");
        }

        assert_eq!(a.count(), iterations);
        assert!(nodes_for_height(a.height() + 1) > iterations);

        // Flush and regenerate amt
        let c = a.flush().unwrap();
        let mut new_amt = AMT::load(&db, c).unwrap();

        for i in 0..iterations {
            assert_get(&mut new_amt, i, &"foo foo bar");
        }
    }

    #[test]
    fn chaos() {
        let db = db::MemoryDB::default();
        let mut _a = AMT::new(&db);
    }

    #[test]
    fn bulk_insert_delete() {
        let db = db::MemoryDB::default();
        let mut _a = AMT::new(&db);
    }

    #[test]
    fn delete() {
        let db = db::MemoryDB::default();
        let mut _a = AMT::new(&db);
    }

    #[test]
    fn delete_first_entry() {
        let db = db::MemoryDB::default();
        let mut _a = AMT::new(&db);
    }

    #[test]
    fn delete_reduce_height() {
        let db = db::MemoryDB::default();
        let mut _a = AMT::new(&db);
    }

    #[test]
    fn loop_set_get() {
        let db = db::MemoryDB::default();
        let mut _a = AMT::new(&db);
    }
}
