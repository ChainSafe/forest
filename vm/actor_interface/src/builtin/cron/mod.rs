// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::load_actor_state;
use ipld_blockstore::BlockStore;
use serde::Serialize;
use vm::ActorState;

/// Cron actor address.
pub static ADDRESS: &actorv4::CRON_ACTOR_ADDR = &actorv4::CRON_ACTOR_ADDR;

/// Cron actor method.
pub type Method = actorv4::cron::Method;

/// Cron actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V0(actorv0::cron::State),
    V2(actorv2::cron::State),
    V3(actorv3::cron::State),
    V4(actorv4::cron::State),
    V5(actorv5::cron::State),
    V6(actorv6::cron::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        load_actor_state!(store, actor, CRON_ACTOR_CODE_ID)
    }
}
