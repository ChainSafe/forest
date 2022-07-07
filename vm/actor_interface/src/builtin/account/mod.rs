// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use cid::multihash::MultihashDigest;
use cid::Cid;
use ipld_blockstore::BlockStore;
use serde::Serialize;
use vm::ActorState;

use anyhow::Context;

/// Account actor method.
pub type Method = fil_actor_account_v8::Method;

/// Account actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    // V7(fil_actor_account_v7::State),
    V8(fil_actor_account_v8::State),
}

pub fn account_cid_v7() -> Cid {
    cid::Cid::new_v1(cid::RAW, cid::Code::Identity.digest(b"fil/7/account"))
}

pub fn account_cid_v8() -> Cid {
    Cid::try_from("bafk2bzacecruossn66xqbeutqx5r4k2kjzgd43frmwd4qkw6haez44ubvvpxo").unwrap()
}
pub fn account_cid_v8_mainnet() -> Cid {
    Cid::try_from("bafk2bzacedudbf7fc5va57t3tmo63snmt3en4iaidv4vo3qlyacbxaa6hlx6y").unwrap()
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        // if actor.code == account_cid_v7() {
        //     Ok(store
        //         .get_anyhow(&actor.state)?
        //         .map(State::V7)
        //         .context("Actor state doesn't exist in store")?)
        // } else
        if actor.code == account_cid_v8() || actor.code == account_cid_v8_mainnet() {
            Ok(store
                .get_anyhow(&actor.state)?
                .map(State::V8)
                .context("Actor state doesn't exist in store")?)
        } else {
            Err(anyhow::anyhow!("Unknown account actor code {}", actor.code))
        }
    }

    pub fn pubkey_address(&self) -> Address {
        match self {
            // State::V7(st) => st.address,
            State::V8(st) => st.address,
        }
    }
}
