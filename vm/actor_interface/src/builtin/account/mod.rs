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
pub type Method = fil_actor_account_v7::Method;

/// Account actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    // V0(actorv0::account::State),
    // V2(actorv2::account::State),
    // V3(actorv3::account::State),
    // V4(actorv4::account::State),
    // V5(actorv5::account::State),
    // V6(actorv6::account::State),
    V7(fil_actor_account_v7::State),
}

pub fn account_cid_v7() -> Cid {
    cid::Cid::new_v1(cid::RAW, cid::Code::Identity.digest(b"fil/7/account"))
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        if actor.code == account_cid_v7() {
            Ok(store
                .get_anyhow(&actor.state)?
                .map(State::V7)
                .context("Actor state doesn't exist in store")?)
        } else {
            Err(anyhow::anyhow!("Unknown account actor code {}", actor.code))
        }
    }

    pub fn pubkey_address(&self) -> Address {
        match self {
            // State::V0(st) => st.address,
            // State::V2(st) => st.address,
            // State::V3(st) => st.address,
            // State::V4(st) => st.address,
            // State::V5(st) => st.address,
            // State::V6(st) => st.address,
            State::V7(st) => st.address,
        }
    }
}
