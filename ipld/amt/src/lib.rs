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

// TODO move tests to folder in crate
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor() {
        AMT::new(&db::MemoryDB::default());
    }

    #[test]
    fn basic_get_set() {}

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
