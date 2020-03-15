// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

mod builtin;

pub use self::builtin::*;
pub use vm::{ActorID, ActorState, Serialized};

use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;

const HAMT_BIT_WIDTH: u8 = 5;

/// Used when invocation requires parameters to be an empty array of bytes
#[inline]
fn assert_empty_params(params: &Serialized) {
    params.deserialize::<[u8; 0]>().unwrap();
}

/// Empty return is an empty serialized array
#[inline]
fn empty_return() -> Serialized {
    Serialized::serialize::<[u8; 0]>([]).unwrap()
}

/// Create a map
#[inline]
fn make_map<BS: BlockStore>(store: &'_ BS) -> Hamt<'_, String, BS> {
    Hamt::new_with_bit_width(store, HAMT_BIT_WIDTH)
}
