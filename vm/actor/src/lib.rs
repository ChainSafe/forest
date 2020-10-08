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
pub use vm::{actor_error, ActorError, ActorState, DealID, ExitCode, Serialized, TokenAmount};

use cid::Cid;
use fil_types::HAMT_BIT_WIDTH;
use ipld_blockstore::BlockStore;
use ipld_hamt::{BytesKey, Error as HamtError, Hamt};
use num_bigint::BigInt;
use serde::{de::DeserializeOwned, Serialize};
use unsigned_varint::decode::Error as UVarintError;

lazy_static! {
    /// The maximum supply of Filecoin that will ever exist (in token units)
    pub static ref TOTAL_FILECOIN: TokenAmount = TokenAmount::from(2_000_000_000) * TOKEN_PRECISION;
}

/// Number of token units in an abstract "FIL" token.
/// The network works purely in the indivisible token amounts.
/// This constant converts to a fixed decimal with more human-friendly scale.
const TOKEN_PRECISION: u64 = 1_000_000_000_000_000_000;

/// Map type to be used within actors. The underlying type is a hamt.
pub type Map<'bs, BS, V> = Hamt<'bs, BS, V, BytesKey>;

/// Deal weight
type DealWeight = BigInt;

/// Used when invocation requires parameters to be an empty array of bytes
fn check_empty_params(params: &Serialized) -> Result<(), ActorError> {
    if !params.is_empty() {
        Err(actor_error!(ErrSerialization;
                "params expected to be empty, was: {}", base64::encode(params.bytes())))
    } else {
        Ok(())
    }
}

/// Create a hamt configured with constant bit width.
#[inline]
pub fn make_map<BS, V>(store: &'_ BS) -> Map<'_, BS, V>
where
    BS: BlockStore,
    V: DeserializeOwned + Serialize + Clone,
{
    Hamt::<_, V>::new_with_bit_width(store, HAMT_BIT_WIDTH)
}

/// Create a map with a root cid.
#[inline]
pub fn make_map_with_root<'bs, BS, V>(
    root: &Cid,
    store: &'bs BS,
) -> Result<Map<'bs, BS, V>, HamtError>
where
    BS: BlockStore,
    V: DeserializeOwned + Serialize + Clone,
{
    Hamt::<_, V>::load_with_bit_width(root, store, HAMT_BIT_WIDTH)
}

pub fn u64_key(k: u64) -> BytesKey {
    let mut bz = unsigned_varint::encode::u64_buffer();
    let slice = unsigned_varint::encode::u64(k, &mut bz);
    slice.to_vec().into()
}

pub fn parse_uint_key(s: &[u8]) -> Result<u64, UVarintError> {
    let (v, _) = unsigned_varint::decode::u64(s)?;
    Ok(v)
}
