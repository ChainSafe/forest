// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

// workaround for a compiler bug, see https://github.com/rust-lang/rust/issues/55779
extern crate serde;

mod builtin;
mod util;

pub use self::builtin::*;
pub use self::util::*;
pub use vm::{ActorState, DealID, Serialized};

use encoding::Error as EncodingError;
use ipld_blockstore::BlockStore;
use ipld_hamt::{BytesKey, Hamt};
use num_bigint::BigUint;
use unsigned_varint::decode::Error as UVarintError;

const HAMT_BIT_WIDTH: u8 = 5;

type EmptyType = [u8; 0];
const EMPTY_VALUE: EmptyType = [];

/// Deal weight
type DealWeight = BigUint;

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

pub fn u64_key(k: u64) -> BytesKey {
    let mut bz = unsigned_varint::encode::u64_buffer();
    unsigned_varint::encode::u64(k, &mut bz);
    bz.to_vec().into()
}

pub fn parse_uint_key(s: &[u8]) -> Result<u64, UVarintError> {
    let (v, _) = unsigned_varint::decode::u64(s)?;
    Ok(v)
}
