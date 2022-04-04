// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::load_state;
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
        load_state!(
            store,
            actor,
            (actorv6::ACCOUNT_ACTOR_CODE_ID, State::V6),
            (actorv5::ACCOUNT_ACTOR_CODE_ID, State::V5),
            (actorv4::ACCOUNT_ACTOR_CODE_ID, State::V4),
            (actorv3::ACCOUNT_ACTOR_CODE_ID, State::V3),
            (actorv2::ACCOUNT_ACTOR_CODE_ID, State::V2),
            (actorv0::ACCOUNT_ACTOR_CODE_ID, State::V0)
        )
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
