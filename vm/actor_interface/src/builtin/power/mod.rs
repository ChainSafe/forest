// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_types::StoragePower;
use ipld_blockstore::BlockStore;
use serde::Serialize;
use std::error::Error;
use vm::ActorState;

/// Power actor address.
pub static ADDRESS: &actorv2::STORAGE_POWER_ACTOR_ADDR = &actorv2::STORAGE_POWER_ACTOR_ADDR;

/// Power actor method.
pub type Method = actorv2::power::Method;

/// Power actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V0(actorv0::power::State),
    V2(actorv2::power::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> Result<Option<State>, Box<dyn Error>>
    where
        BS: BlockStore,
    {
        if actor.code == *actorv0::POWER_ACTOR_CODE_ID {
            Ok(store.get(&actor.state)?.map(State::V0))
        } else if actor.code == *actorv2::POWER_ACTOR_CODE_ID {
            Ok(store.get(&actor.state)?.map(State::V2))
        } else {
            Err(format!("Unknown actor code {}", actor.code).into())
        }
    }

    /// Consume state to return just total quality adj power
    pub fn into_total_quality_adj_power(self) -> StoragePower {
        match self {
            State::V0(st) => st.total_quality_adj_power,
            State::V2(st) => st.total_quality_adj_power,
        }
    }
}
