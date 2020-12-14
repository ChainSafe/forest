// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! HAMT crate for use as rust IPLD data structure
//!
//! [Data structure reference](https://github.com/ipld/specs/blob/51fab05b4fe4930d3d851d50cc1e5f1a02092deb/data-structures/hashmap.md)
//!
//! Implementation based off the work @dignifiedquire started [here](https://github.com/dignifiedquire/rust-hamt-ipld). This implementation matched the rust HashMap interface very closely, but came at the cost of saving excess values to the database and requiring unsafe code to update the cache from the underlying store as well as discarding any errors that came in any operations. The function signatures that exist are based on this, but refactored to match the spec more closely and match the necessary implementation.
//!
//! The Hamt is a data structure that mimmics a HashMap which has the features of being sharded, persisted, and indexable by a Cid. The Hamt supports a variable bit width to adjust the amount of possible pointers that can exist at each height of the tree. Hamt can be modified at any point, but the underlying values are only persisted to the store when the [flush](struct.Hamt.html#method.flush) is called.

mod bitfield;
mod error;
mod hamt;
mod hash;
mod hash_algorithm;
mod hash_bits;
mod node;
mod pointer;

pub use self::error::Error;
pub use self::hamt::Hamt;
pub use self::hash::*;
pub use self::hash_algorithm::*;

pub use forest_hash_utils::{BytesKey, Hash};
use serde::{Deserialize, Serialize};

const MAX_ARRAY_WIDTH: usize = 3;

/// Default bit width for indexing a hash at each depth level
const DEFAULT_BIT_WIDTH: u32 = 8;

type HashedKey = [u8; 32];

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct KeyValuePair<K, V>(K, V);

impl<K, V> KeyValuePair<K, V> {
    pub fn key(&self) -> &K {
        &self.0
    }
    pub fn value(&self) -> &V {
        &self.1
    }
}

impl<K, V> KeyValuePair<K, V> {
    pub fn new(key: K, value: V) -> Self {
        KeyValuePair(key, value)
    }
}
