// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ipld_blockstore::BlockStore;
use serde::Serialize;
use vm::ActorState;

/// Cron actor address.
pub static ADDRESS: &fil_actors_runtime_v7::builtin::singletons::CRON_ACTOR_ADDR =
    &fil_actors_runtime_v7::builtin::singletons::CRON_ACTOR_ADDR;

/// Cron actor method.
pub type Method = fil_actor_cron_v7::Method;

/// Cron actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {}

impl State {
    pub fn load<BS>(_store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        Err(anyhow::anyhow!("Unknown cron actor code {}", actor.code))
    }
}
