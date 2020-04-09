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
mod hash_bits;
mod node;
mod pointer;

pub use self::error::Error;
pub use self::hamt::Hamt;
pub use self::hash::*;

use forest_ipld::Ipld;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Borrow;
use std::hash::Hasher;
use std::ops::Deref;

const MAX_ARRAY_WIDTH: usize = 3;

/// Default bit width for indexing a hash at each depth level
pub const DEFAULT_BIT_WIDTH: u8 = 8;

type HashedKey = [u8; 16];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct KeyValuePair<K>(K, Ipld);

impl<K> KeyValuePair<K> {
    pub fn key(&self) -> &K {
        &self.0
    }
}

impl<K> KeyValuePair<K> {
    pub fn new(key: K, value: Ipld) -> Self {
        KeyValuePair(key, value)
    }
}

/// Key type to be used to isolate usage of unsafe code and allow non utf-8 bytes to be
/// serialized as a string.
#[derive(Eq, PartialOrd, Clone, Debug)]
pub struct BytesKey(pub Vec<u8>);

impl PartialEq for BytesKey {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Hash for BytesKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.0);
    }
}

impl Borrow<[u8]> for BytesKey {
    fn borrow(&self) -> &[u8] {
        &self.0
    }
}

impl Borrow<Vec<u8>> for BytesKey {
    fn borrow(&self) -> &Vec<u8> {
        &self.0
    }
}

impl Deref for BytesKey {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Serialize for BytesKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serde_bytes::Serialize::serialize(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for BytesKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self(serde_bytes::Deserialize::deserialize(deserializer)?))
    }
}

impl From<Vec<u8>> for BytesKey {
    fn from(bz: Vec<u8>) -> Self {
        BytesKey(bz)
    }
}

impl From<&[u8]> for BytesKey {
    fn from(s: &[u8]) -> Self {
        Self(s.to_vec())
    }
}

impl From<&str> for BytesKey {
    fn from(s: &str) -> Self {
        Self::from(s.as_bytes())
    }
}
