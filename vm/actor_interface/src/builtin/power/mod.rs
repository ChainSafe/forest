// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::FilterEstimate;
use cid::Cid;
use fil_actors_runtime_v8::runtime::Policy;
use forest_fil_types::StoragePower;
use forest_ipld_blockstore::BlockStore;
use forest_ipld_blockstore::BlockStoreExt;
use forest_json::bigint::json;
use forest_vm::TokenAmount;
use fvm::state_tree::ActorState;
use fvm_shared::address::Address;
use serde::{Deserialize, Serialize};

use anyhow::Context;

/// Power actor address.
// TODO: Select address based on actors version
pub const ADDRESS: Address = Address::new_id(4);

/// Power actor method.
// TODO: Select method based on actors version
pub type Method = fil_actor_power_v8::Method;

pub fn is_v8_power_cid(cid: &Cid) -> bool {
    let known_cids = vec![
        // calibnet
        Cid::try_from("bafk2bzacecpwr4mynn55bg5hrlns3osvg7sty3rca6zlai3vl52vbbjk7ulfa").unwrap(),
        // mainnet
        Cid::try_from("bafk2bzacebjvqva6ppvysn5xpmiqcdfelwbbcxmghx5ww6hr37cgred6dyrpm").unwrap(),
        // devnet
        Cid::try_from("bafk2bzaceb45l6zhgc34n6clz7xnvd7ek55bhw46q25umuje34t6kroix6hh6").unwrap(),
    ];
    known_cids.contains(cid)
}

/// Power actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V8(fil_actor_power_v8::State),
}

/// Converts any `FilterEstimate`, e.g. `actorv0::util::smooth::FilterEstimate` type into
/// generalized one `crate::FilterEstimate`.
macro_rules! convert_filter_estimate {
    ($from:expr) => {
        FilterEstimate {
            position: $from.position.clone(),
            velocity: $from.velocity.clone(),
        }
    };
}
impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        if is_v8_power_cid(&actor.code) {
            return store
                .get_obj(&actor.state)?
                .map(State::V8)
                .context("Actor state doesn't exist in store");
        }
        Err(anyhow::anyhow!("Unknown power actor code {}", actor.code))
    }

    /// Consume state to return just total quality adj power
    pub fn into_total_quality_adj_power(self) -> StoragePower {
        match self {
            State::V8(st) => st.total_quality_adj_power,
        }
    }

    /// Returns the total power claim.
    pub fn total_power(&self) -> Claim {
        match self {
            State::V8(st) => Claim {
                raw_byte_power: st.total_raw_byte_power.clone(),
                quality_adj_power: st.total_quality_adj_power.clone(),
            },
        }
    }

    /// Consume state to return total locked funds
    pub fn into_total_locked(self) -> TokenAmount {
        match self {
            State::V8(st) => st.into_total_locked(),
        }
    }

    /// Loads power for a given miner, if exists.
    pub fn miner_power<BS: BlockStore>(
        &self,
        s: &BS,
        miner: &Address,
    ) -> anyhow::Result<Option<Claim>> {
        match self {
            State::V8(st) => Ok(st.miner_power(&s, miner)?.map(From::from)),
        }
    }

    /// Loads power for a given miner, if exists.
    pub fn list_all_miners<BS: BlockStore>(&self, _s: &BS) -> anyhow::Result<Vec<Address>> {
        unimplemented!()
    }

    /// Checks power actor state for if miner meets minimum consensus power.
    pub fn miner_nominal_power_meets_consensus_minimum<BS: BlockStore>(
        &self,
        policy: &Policy,
        s: &BS,
        miner: &Address,
    ) -> anyhow::Result<bool> {
        match self {
            State::V8(st) => st.miner_nominal_power_meets_consensus_minimum(policy, &s, miner),
        }
    }

    /// Returns `this_epoch_qa_power_smoothed` from the state.
    pub fn total_power_smoothed(&self) -> FilterEstimate {
        match self {
            State::V8(st) => convert_filter_estimate!(st.this_epoch_qa_power_smoothed),
        }
    }

    /// Returns total locked funds
    pub fn total_locked(&self) -> TokenAmount {
        match self {
            State::V8(st) => st.total_pledge_collateral.clone(),
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct Claim {
    /// Sum of raw byte power for a miner's sectors.
    #[serde(with = "json")]
    pub raw_byte_power: StoragePower,
    /// Sum of quality adjusted power for a miner's sectors.
    #[serde(with = "json")]
    pub quality_adj_power: StoragePower,
}

impl From<fil_actor_power_v8::Claim> for Claim {
    fn from(cl: fil_actor_power_v8::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}
