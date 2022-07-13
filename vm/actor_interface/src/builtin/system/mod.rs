// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use ipld_blockstore::BlockStore;
use serde::Serialize;
use vm::ActorState;

/// System actor address.
pub const ADDRESS: Address = Address::new_id(0);

/// System actor method.
pub type Method = fil_actor_system_v8::Method;

/// System actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {}

impl State {
    pub fn load<BS>(_store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        Err(anyhow::anyhow!("Unknown system actor code {}", actor.code))
    }
}
