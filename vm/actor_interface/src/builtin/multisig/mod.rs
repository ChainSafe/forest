// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_vm::ActorState;
use ipld_blockstore::BlockStore;
use serde::Serialize;

/// Multisig actor method.
pub type Method = fil_actor_multisig_v8::Method;

/// Multisig actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {}

impl State {
    pub fn load<BS>(_store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        Err(anyhow::anyhow!(
            "Unknown multisig actor code {}",
            actor.code
        ))
    }
}
