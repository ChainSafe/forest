// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ipld_blockstore::BlockStore;
use serde::Serialize;
use std::error::Error;
use vm::ActorState;

/// Multisig actor method.
pub type Method = actorv3::multisig::Method;

/// Multisig actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V0(actorv0::multisig::State),
    V2(actorv2::multisig::State),
    V3(actorv3::multisig::State),
    V4(actorv4::multisig::State),
    V5(actorv5::multisig::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> Result<State, Box<dyn Error>>
    where
        BS: BlockStore,
    {
        if actor.code == *actorv0::MULTISIG_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V0)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv2::MULTISIG_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V2)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv3::MULTISIG_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V3)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv4::MULTISIG_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V4)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv5::MULTISIG_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V5)
                .ok_or("Actor state doesn't exist in store")?)
        } else {
            Err(format!("Unknown actor code {}", actor.code).into())
        }
    }
}
