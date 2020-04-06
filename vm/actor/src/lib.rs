// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

mod builtin;
mod util;

pub use self::builtin::*;
pub use self::util::*;
pub use vm::{ActorState, DealID, Serialized};

use encoding::Error as EncodingError;
use ipld_blockstore::BlockStore;
use ipld_hamt::{Hamt, Hash};
use num_bigint::BigInt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Borrow;
use std::hash::Hasher;
use std::ops::Deref;
use std::str;
use unsigned_varint::decode::Error as UVarintError;

const HAMT_BIT_WIDTH: u8 = 5;

type EmptyType = [u8; 0];
const EMPTY_VALUE: EmptyType = [];

/// Storage power unit, could possibly be a BigUint
type StoragePower = BigInt;

/// Deal weight
type DealWeight = BigInt;

/// Used when invocation requires parameters to be an empty array of bytes
#[inline]
fn check_empty_params(params: &Serialized) -> Result<(), EncodingError> {
    params.deserialize::<[u8; 0]>().map(|_| ())
}

/// Create a map
#[inline]
fn make_map<BS: BlockStore>(store: &'_ BS) -> Hamt<'_, BytesKey, BS> {
    Hamt::new_with_bit_width(store, HAMT_BIT_WIDTH)
}

pub fn deal_key(d: DealID) -> BytesKey {
    let mut bz = unsigned_varint::encode::u64_buffer();
    unsigned_varint::encode::u64(d, &mut bz);
    bz.to_vec().into()
}

pub fn parse_uint_key(s: &[u8]) -> Result<u64, UVarintError> {
    let (v, _) = unsigned_varint::decode::u64(s)?;
    Ok(v)
}

/// Key type to be used to isolate usage of unsafe code and allow non utf-8 bytes to be
/// serialized as a string.
// TODO revisit to change serialization in spec or find a non-unsafe way to do this
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
        unsafe { str::from_utf8_unchecked(&self.0) }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BytesKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let non_utf: String = Deserialize::deserialize(deserializer)?;
        Ok(BytesKey(non_utf.into_bytes()))
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
