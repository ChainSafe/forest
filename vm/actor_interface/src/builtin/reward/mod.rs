// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::FilterEstimate;
use cid::multihash::MultihashDigest;
use fil_types::StoragePower;
use ipld_blockstore::BlockStore;
use ipld_blockstore::BlockStoreExt;
use serde::Serialize;
use vm::{ActorState, TokenAmount};

use anyhow::Context;

/// Reward actor address.
pub static ADDRESS: &fil_actors_runtime_v7::builtin::singletons::REWARD_ACTOR_ADDR =
    &fil_actors_runtime_v7::builtin::singletons::REWARD_ACTOR_ADDR;

/// Reward actor method.
pub type Method = fil_actor_reward_v7::Method;

/// Reward actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V7(fil_actor_reward_v7::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        if actor.code == cid::Cid::new_v1(cid::RAW, cid::Code::Identity.digest(b"fil/7/reward")) {
            Ok(store
                .get_obj(&actor.state)?
                .map(State::V7)
                .context("Actor state doesn't exist in store")?)
        } else {
            Err(anyhow::anyhow!("Unknown reward actor code {}", actor.code))
        }
    }

    /// Consume state to return just storage power reward
    pub fn into_total_storage_power_reward(self) -> StoragePower {
        match self {
            State::V7(st) => st.into_total_storage_power_reward(),
        }
    }

    pub fn pre_commit_deposit_for_power(
        &self,
        _network_qa_power: FilterEstimate,
        _sector_weight: &StoragePower,
    ) -> TokenAmount {
        match self {
            State::V7(_st) => todo!(),
        }
    }

    pub fn initial_pledge_for_power(
        &self,
        _sector_weight: &StoragePower,
        _network_total_pledge: &TokenAmount,
        _network_qa_power: FilterEstimate,
        _circ_supply: &TokenAmount,
    ) -> TokenAmount {
        match self {
            State::V7(_st) => todo!(),
        }
    }
}
