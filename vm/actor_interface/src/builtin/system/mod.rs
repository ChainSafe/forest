// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ipld_blockstore::BlockStore;
use serde::Serialize;
use vm::ActorState;

/// System actor address.
pub static ADDRESS: &fil_actors_runtime_v7::builtin::singletons::SYSTEM_ACTOR_ADDR =
    &fil_actors_runtime_v7::builtin::singletons::SYSTEM_ACTOR_ADDR;

/// System actor method.
pub type Method = fil_actor_system_v7::Method;

/// System actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    // V0(actorv0::system::State),
    // V2(actorv2::system::State),
    // V3(actorv3::system::State),
    // V4(actorv4::system::State),
    // V5(actorv5::system::State),
    // V6(actorv6::system::State),
}

impl State {
    pub fn load<BS>(_store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        Err(anyhow::anyhow!("Unknown system actor code {}", actor.code))
    }
}
