// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::FilterEstimate;
use fil_types::StoragePower;
use forest_address::Address;
use forest_cid::Cid;
use forest_vm::{ActorState, TokenAmount};
use ipld_blockstore::BlockStore;
use ipld_blockstore::BlockStoreExt;
use serde::Serialize;

use anyhow::Context;

/// Reward actor address.
pub const ADDRESS: Address = Address::new_id(2);

/// Reward actor method.
pub type Method = fil_actor_reward_v8::Method;

pub fn is_v8_reward_cid(cid: &Cid) -> bool {
    let known_cids = vec![
        // calibnet
        Cid::try_from("bafk2bzaceayah37uvj7brl5no4gmvmqbmtndh5raywuts7h6tqbgbq2ge7dhu").unwrap(),
        // mainnet
        Cid::try_from("bafk2bzacecwzzxlgjiavnc3545cqqil3cmq4hgpvfp2crguxy2pl5ybusfsbe").unwrap(),
    ];
    known_cids.contains(cid)
}

/// Reward actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V8(fil_actor_reward_v8::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        if is_v8_reward_cid(&actor.code) {
            return store
                .get_obj(&actor.state)?
                .map(State::V8)
                .context("Actor state doesn't exist in store");
        }
        Err(anyhow::anyhow!("Unknown reward actor code {}", actor.code))
    }

    /// Consume state to return just storage power reward
    pub fn into_total_storage_power_reward(self) -> StoragePower {
        match self {
            State::V8(st) => st.into_total_storage_power_reward(),
        }
    }

    pub fn pre_commit_deposit_for_power(
        &self,
        _network_qa_power: FilterEstimate,
        _sector_weight: &StoragePower,
    ) -> TokenAmount {
        todo!()
    }

    pub fn initial_pledge_for_power(
        &self,
        _sector_weight: &StoragePower,
        _network_total_pledge: &TokenAmount,
        _network_qa_power: FilterEstimate,
        _circ_supply: &TokenAmount,
    ) -> TokenAmount {
        todo!()
    }
}
