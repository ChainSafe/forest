// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

mod amt;
mod block_store;
mod error;
mod node;
mod root;

pub use self::amt::*;
pub use self::block_store::*;
pub use self::error::*;
pub use self::node::*;
pub use self::root::*;

const WIDTH: usize = 8;

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
    }

    #[test]
    fn out_of_range() {}

    #[test]
    fn expand() {}

    #[test]
    fn bulk_insert() {}

    #[test]
    fn chaos() {}

    #[test]
    fn bulk_insert_delete() {}

    #[test]
    fn delete() {}

    #[test]
    fn delete_first_entry() {}

    #[test]
    fn delete_reduce_height() {}

    #[test]
    fn loop_set_get() {}
}
