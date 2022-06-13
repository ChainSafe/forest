// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::load_actor_state;
use ipld_blockstore::BlockStore;
use serde::Serialize;
use vm::ActorState;

/// System actor address.
pub static ADDRESS: &actorv3::SYSTEM_ACTOR_ADDR = &actorv3::SYSTEM_ACTOR_ADDR;

/// System actor method.
pub type Method = actorv3::system::Method;

/// System actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V0(actorv0::system::State),
    V2(actorv2::system::State),
    V3(actorv3::system::State),
    V4(actorv4::system::State),
    V5(actorv5::system::State),
    V6(actorv6::system::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        load_actor_state!(store, actor, SYSTEM_ACTOR_CODE_ID)
    }
}
