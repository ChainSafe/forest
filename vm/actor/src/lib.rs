// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

// workaround for a compiler bug, see https://github.com/rust-lang/rust/issues/55779
extern crate serde;

mod builtin;
pub mod util;

pub use self::builtin::*;
pub use self::util::*;
pub use ipld_amt;
pub use ipld_hamt;
pub use vm::{
    actor_error, ActorError, ActorState, DealID, ExitCode, MethodNum, Serialized, TokenAmount,
};

use cid::Cid;
use fil_types::HAMT_BIT_WIDTH;
use ipld_blockstore::BlockStore;
use ipld_hamt::{BytesKey, Error as HamtError, Hamt};
use num_bigint::BigInt;
use runtime::{ActorCode, Runtime};
use serde::{de::DeserializeOwned, Serialize};
use unsigned_varint::decode::Error as UVarintError;

/// Map type to be used within actors. The underlying type is a hamt.
pub type Map<'bs, BS, V> = Hamt<'bs, BS, V, BytesKey>;

/// Deal weight
pub type DealWeight = BigInt;

/// Create a hamt with a custom bitwidth.
#[inline]
pub fn make_empty_map<BS, V>(store: &'_ BS, bitwidth: u32) -> Map<'_, BS, V>
where
    BS: BlockStore,
    V: DeserializeOwned + Serialize,
{
    Map::<_, V>::new_with_bit_width(store, bitwidth)
}

/// Create a map with a root cid.
#[inline]
pub fn make_map_with_root<'bs, BS, V>(
    root: &Cid,
    store: &'bs BS,
) -> Result<Map<'bs, BS, V>, HamtError>
where
    BS: BlockStore,
    V: DeserializeOwned + Serialize,
{
    Map::<_, V>::load_with_bit_width(root, store, HAMT_BIT_WIDTH)
}

/// Create a map with a root cid.
#[inline]
pub fn make_map_with_root_and_bitwidth<'bs, BS, V>(
    root: &Cid,
    store: &'bs BS,
    bitwidth: u32,
) -> Result<Map<'bs, BS, V>, HamtError>
where
    BS: BlockStore,
    V: DeserializeOwned + Serialize,
{
    Map::<_, V>::load_with_bit_width(root, store, bitwidth)
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

pub fn invoke_code<RT, BS>(
    code: &Cid,
    rt: &mut RT,
    method_num: MethodNum,
    params: &Serialized,
) -> Option<Result<Serialized, ActorError>>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if code == &*SYSTEM_ACTOR_CODE_ID {
        Some(system::Actor::invoke_method(rt, method_num, params))
    } else if code == &*INIT_ACTOR_CODE_ID {
        Some(init::Actor::invoke_method(rt, method_num, params))
    } else if code == &*CRON_ACTOR_CODE_ID {
        Some(cron::Actor::invoke_method(rt, method_num, params))
    } else if code == &*ACCOUNT_ACTOR_CODE_ID {
        Some(account::Actor::invoke_method(rt, method_num, params))
    } else if code == &*POWER_ACTOR_CODE_ID {
        Some(power::Actor::invoke_method(rt, method_num, params))
    } else if code == &*MINER_ACTOR_CODE_ID {
        Some(miner::Actor::invoke_method(rt, method_num, params))
    } else if code == &*MARKET_ACTOR_CODE_ID {
        Some(market::Actor::invoke_method(rt, method_num, params))
    } else if code == &*PAYCH_ACTOR_CODE_ID {
        Some(paych::Actor::invoke_method(rt, method_num, params))
    } else if code == &*MULTISIG_ACTOR_CODE_ID {
        Some(multisig::Actor::invoke_method(rt, method_num, params))
    } else if code == &*REWARD_ACTOR_CODE_ID {
        Some(reward::Actor::invoke_method(rt, method_num, params))
    } else if code == &*VERIFREG_ACTOR_CODE_ID {
        Some(verifreg::Actor::invoke_method(rt, method_num, params))
    } else {
        None
    }
}
