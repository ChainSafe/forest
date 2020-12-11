// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_types::StoragePower;
use ipld_blockstore::BlockStore;
use serde::Serialize;
use std::error::Error;
use vm::ActorState;

/// Reward actor address.
pub static ADDRESS: &actorv2::REWARD_ACTOR_ADDR = &actorv2::REWARD_ACTOR_ADDR;

/// Reward actor method.
pub type Method = actorv2::reward::Method;

/// Reward actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V0(actorv0::reward::State),
    V2(actorv2::reward::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> Result<State, Box<dyn Error>>
    where
        BS: BlockStore,
    {
        if actor.code == *actorv0::REWARD_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V0)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv2::REWARD_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V2)
                .ok_or("Actor state doesn't exist in store")?)
        } else {
            Err(format!("Unknown actor code {}", actor.code).into())
        }
    }

    /// Consume state to return just storage power reward
    pub fn into_total_storage_power_reward(self) -> StoragePower {
        match self {
            State::V0(st) => st.into_total_storage_power_reward(),
            State::V2(st) => st.into_total_storage_power_reward(),
        }
    }
}
