// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::load_state;
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
        load_state!(
            store,
            actor,
            (actorv6::CRON_ACTOR_CODE_ID, State::V6),
            (actorv5::CRON_ACTOR_CODE_ID, State::V5),
            (actorv4::CRON_ACTOR_CODE_ID, State::V4),
            (actorv3::CRON_ACTOR_CODE_ID, State::V3),
            (actorv2::CRON_ACTOR_CODE_ID, State::V2),
            (actorv0::CRON_ACTOR_CODE_ID, State::V0)
        )
    }
}
