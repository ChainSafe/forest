// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::FilterEstimate;
use address::Address;
use cid::multihash::MultihashDigest;
use fil_types::StoragePower;

use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser::json;
use serde::{Deserialize, Serialize};
use vm::{ActorState, TokenAmount};

use anyhow::Context;

/// Power actor address.
/// TODO: Select based on actors version
pub static ADDRESS: &fil_actors_runtime_v7::builtin::singletons::STORAGE_POWER_ACTOR_ADDR =
    &fil_actors_runtime_v7::builtin::singletons::STORAGE_POWER_ACTOR_ADDR;

/// Power actor method.
/// TODO: Select based on actor version
pub type Method = fil_actor_power_v7::Method;

/// Power actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V7(fil_actor_power_v7::State),
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
        if actor.code
            == cid::Cid::new_v1(cid::RAW, cid::Code::Identity.digest(b"fil/7/storagepower"))
        {
            Ok(store
                .get_anyhow(&actor.state)?
                .map(State::V7)
                .context("Actor state doesn't exist in store")?)
        } else {
            Err(anyhow::anyhow!("Unknown power actor code {}", actor.code))
        }
    }

    /// Consume state to return just total quality adj power
    pub fn into_total_quality_adj_power(self) -> StoragePower {
        match self {
            State::V7(st) => st.total_quality_adj_power,
        }
    }

    /// Returns the total power claim.
    pub fn total_power(&self) -> Claim {
        match self {
            State::V7(st) => Claim {
                raw_byte_power: st.total_raw_byte_power.clone(),
                quality_adj_power: st.total_quality_adj_power.clone(),
            },
        }
    }

    /// Consume state to return total locked funds
    pub fn into_total_locked(self) -> TokenAmount {
        match self {
            State::V7(st) => st.into_total_locked(),
        }
    }

    /// Loads power for a given miner, if exists.
    pub fn miner_power<BS: BlockStore>(
        &self,
        s: &BS,
        miner: &Address,
    ) -> anyhow::Result<Option<Claim>> {
        match self {
            State::V7(st) => {
                let fvm_store = ipld_blockstore::FvmRefStore::new(s);
                Ok(st.miner_power(&fvm_store, miner)?.map(From::from))
            }
        }
    }

    /// Loads power for a given miner, if exists.
    pub fn list_all_miners<BS: BlockStore>(&self, _s: &BS) -> anyhow::Result<Vec<Address>> {
        unimplemented!()
    }

    /// Checks power actor state for if miner meets minimum consensus power.
    pub fn miner_nominal_power_meets_consensus_minimum<BS: BlockStore>(
        &self,
        s: &BS,
        miner: &Address,
    ) -> anyhow::Result<bool> {
        match self {
            State::V7(st) => {
                let fvm_store = ipld_blockstore::FvmRefStore::new(s);
                Ok(st
                    .miner_nominal_power_meets_consensus_minimum(&fvm_store, miner)
                    .expect("FIXME"))
            }
        }
    }

    /// Returns this_epoch_qa_power_smoothed from the state.
    pub fn total_power_smoothed(&self) -> FilterEstimate {
        match self {
            State::V7(st) => convert_filter_estimate!(st.this_epoch_qa_power_smoothed),
        }
    }

    /// Returns total locked funds
    pub fn total_locked(&self) -> TokenAmount {
        match self {
            State::V7(st) => st.total_pledge_collateral.clone(),
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

impl From<fil_actor_power_v7::Claim> for Claim {
    fn from(cl: fil_actor_power_v7::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}
