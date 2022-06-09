// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::load_actor_state;
use ipld_blockstore::BlockStore;
use serde::Serialize;
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
    V6(actorv6::multisig::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        load_actor_state!(store, actor, MULTISIG_ACTOR_CODE_ID)
    }
}
