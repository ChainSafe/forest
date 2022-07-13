// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use cid::Cid;
use ipld_blockstore::BlockStore;
use serde::Serialize;
use vm::ActorState;

use anyhow::Context;
use ipld_blockstore::BlockStoreExt;

/// Account actor method.
pub type Method = fil_actor_account_v8::Method;

/// Account actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V8(fil_actor_account_v8::State),
}

pub fn is_v8_account_cid(cid: &Cid) -> bool {
    let known_cids = vec![
        // calibnet
        Cid::try_from("bafk2bzacecruossn66xqbeutqx5r4k2kjzgd43frmwd4qkw6haez44ubvvpxo").unwrap(),
        // mainnet
        Cid::try_from("bafk2bzacedudbf7fc5va57t3tmo63snmt3en4iaidv4vo3qlyacbxaa6hlx6y").unwrap(),
    ];
    known_cids.contains(cid)
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        if is_v8_account_cid(&actor.code) {
            return store
                .get_obj(&actor.state)?
                .map(State::V8)
                .context("Actor state doesn't exist in store");
        }
        Err(anyhow::anyhow!("Unknown account actor code {}", actor.code))
    }

    pub fn pubkey_address(&self) -> Address {
        match self {
            State::V8(st) => st.address,
        }
    }
}
