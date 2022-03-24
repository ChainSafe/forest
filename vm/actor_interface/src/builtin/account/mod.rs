// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use cid::multihash::MultihashDigest;
use cid::Cid;
use ipld_blockstore::BlockStore;
use serde::Serialize;
use std::error::Error;
use vm::ActorState;

/// Account actor method.
pub type Method = actorv4::account::Method;

/// Account actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V0(actorv0::account::State),
    V2(actorv2::account::State),
    V3(actorv3::account::State),
    V4(actorv4::account::State),
    V5(actorv5::account::State),
    V6(actorv6::account::State),
    V7(fil_actor_account_v7::State),
}

pub fn account_cid_v7() -> Cid {
    cid::Cid::new_v1(cid::RAW, cid::Code::Identity.digest(b"fil/7/account"))
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> Result<State, Box<dyn Error>>
    where
        BS: BlockStore,
    {
        if actor.code == *actorv0::ACCOUNT_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V0)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv2::ACCOUNT_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V2)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv3::ACCOUNT_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V3)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv4::ACCOUNT_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V4)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv5::ACCOUNT_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V5)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv6::ACCOUNT_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V6)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == account_cid_v7() {
            Ok(store
                .get(&actor.state)?
                .map(State::V7)
                .ok_or("Actor state doesn't exist in store")?)
        } else {
            Err(format!("Unknown account actor code {}", actor.code).into())
        }
    }

    pub fn pubkey_address(&self) -> Address {
        match self {
            State::V0(st) => st.address,
            State::V2(st) => st.address,
            State::V3(st) => st.address,
            State::V4(st) => st.address,
            State::V5(st) => st.address,
            State::V6(st) => st.address,
            State::V7(st) => st.address,
        }
    }
}
