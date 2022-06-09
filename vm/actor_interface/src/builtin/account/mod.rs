// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::load_actor_state;
use address::Address;
use ipld_blockstore::BlockStore;
use serde::Serialize;
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
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        load_actor_state!(store, actor, ACCOUNT_ACTOR_CODE_ID)
    }

    pub fn pubkey_address(&self) -> Address {
        match self {
            State::V0(st) => st.address,
            State::V2(st) => st.address,
            State::V3(st) => st.address,
            State::V4(st) => st.address,
            State::V5(st) => st.address,
            State::V6(st) => st.address,
        }
    }
}
