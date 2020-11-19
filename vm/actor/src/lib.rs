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
pub use vm::{
    actor_error, ActorError, ActorState, DealID, ExitCode, MethodNum, Serialized, TokenAmount,
};

use cid::Cid;
use fil_types::{NetworkVersion, HAMT_BIT_WIDTH};
use ipld_blockstore::BlockStore;
use ipld_hamt::{BytesKey, Error as HamtError, Hamt};
use num_bigint::BigInt;
use runtime::{ActorCode, Runtime};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Display;
use unsigned_varint::decode::Error as UVarintError;

/// Map type to be used within actors. The underlying type is a hamt.
pub type Map<'bs, BS, V> = Hamt<'bs, BS, V, BytesKey>;

/// Deal weight
pub type DealWeight = BigInt;

/// Used when invocation requires parameters to be an empty array of bytes
pub fn check_empty_params(params: &Serialized) -> Result<(), ActorError> {
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
    Map::<_, V>::new_with_bit_width(store, HAMT_BIT_WIDTH)
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
    Map::<_, V>::load_with_bit_width(root, store, HAMT_BIT_WIDTH)
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

/// Function to mimmic go implementation's panic handling. They recover from any panics as an exit
/// code defined in the call function of Lotus.
pub(crate) fn actor_assert(
    assertion: bool,
    network_version: NetworkVersion,
    msg: impl Display,
) -> Result<(), ActorError> {
    if !assertion {
        if network_version <= NetworkVersion::V3 {
            Err(actor_error!(
                SysErrSenderInvalid,
                "actors assertion failure: {}",
                msg
            ))
        } else {
            Err(actor_error!(
                SysErrActorPanic,
                "actors assertion failure: {}",
                msg
            ))
        }
    } else {
        Ok(())
    }
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
