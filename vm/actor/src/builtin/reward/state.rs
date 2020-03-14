// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use ipld_blockstore::BlockStore;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::TokenAmount;

pub struct Reward {
    // TODO update to new spec
    pub start_epoch: ChainEpoch,
    pub value: TokenAmount,
    pub release_rate: TokenAmount,
    pub amount_withdrawn: TokenAmount,
}

/// Reward actor state
pub struct State {
    pub reward_map: Cid,
    pub reward_total: TokenAmount,
}

impl State {
    pub fn withdraw_reward<BS: BlockStore>(
        _store: &BS,
        _owner: Address,
        _curr_epoch: ChainEpoch,
    ) -> TokenAmount {
        // TODO
        todo!()
    }
}

impl Serialize for State {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.reward_map, &self.reward_total).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for State {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (reward_map, reward_total) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            reward_map,
            reward_total,
        })
    }
}
