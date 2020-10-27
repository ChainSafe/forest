// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

mod builtin;

pub use self::builtin::*;
pub use actorv1::util::*;
pub use actorv1::{
    check_empty_params, parse_uint_key, u64_key, DealWeight, TOKEN_PRECISION, TOTAL_FILECOIN,
};
pub use vm::{
    actor_error, ActorError, ActorState, DealID, ExitCode, MethodNum, Serialized, TokenAmount,
};

use cid::Cid;
use fil_types::HAMT_BIT_WIDTH;
use ipld_blockstore::BlockStore;
use ipld_hamt::{BytesKey, Error as HamtError, Hamt};
use runtime::{ActorCode, Runtime};
use serde::{de::DeserializeOwned, Serialize};

/// Map type to be used within actors. The underlying type is a hamt.
// TODO needs to use different version of Hamt than v1
pub type Map<'bs, BS, V> = Hamt<'bs, BS, V, BytesKey>;

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
