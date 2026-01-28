// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod ext;

use crate::list_miners_for_state;
use crate::shim::{
    actors::{FilterEstimate, convert::*},
    address::Address,
    econ::TokenAmount,
    runtime::Policy,
    sector::StoragePower,
};
use fvm_ipld_blockstore::Blockstore;
use serde::{Deserialize, Serialize};
use spire_enum::prelude::delegated_enum;

/// Power actor address.
// TODO(forest): https://github.com/ChainSafe/forest/issues/5011
pub const ADDRESS: Address = Address::new_id(4);

/// Power actor method.
// TODO(forest): https://github.com/ChainSafe/forest/issues/5011
pub type Method = fil_actor_power_state::v8::Method;

/// Power actor state.
#[derive(Serialize, Debug)]
#[serde(untagged)]
#[delegated_enum(impl_conversions)]
pub enum State {
    V8(fil_actor_power_state::v8::State),
    V9(fil_actor_power_state::v9::State),
    V10(fil_actor_power_state::v10::State),
    V11(fil_actor_power_state::v11::State),
    V12(fil_actor_power_state::v12::State),
    V13(fil_actor_power_state::v13::State),
    V14(fil_actor_power_state::v14::State),
    V15(fil_actor_power_state::v15::State),
    V16(fil_actor_power_state::v16::State),
    V17(fil_actor_power_state::v17::State),
}

impl State {
    #[allow(clippy::too_many_arguments)]
    pub fn default_latest_version(
        total_raw_byte_power: StoragePower,
        total_bytes_committed: StoragePower,
        total_quality_adj_power: StoragePower,
        total_qa_bytes_committed: StoragePower,
        total_pledge_collateral: fvm_shared4::econ::TokenAmount,
        this_epoch_raw_byte_power: StoragePower,
        this_epoch_quality_adj_power: StoragePower,
        this_epoch_pledge_collateral: fvm_shared4::econ::TokenAmount,
        this_epoch_qa_power_smoothed: fil_actors_shared::v17::builtin::reward::smooth::FilterEstimate,
        miner_count: i64,
        miner_above_min_power_count: i64,
        cron_event_queue: cid::Cid,
        first_cron_epoch: i64,
        claims: cid::Cid,
        proof_validation_batch: Option<cid::Cid>,
        ramp_start_epoch: i64,
        ramp_duration_epochs: u64,
    ) -> Self {
        State::V17(fil_actor_power_state::v17::State {
            total_raw_byte_power,
            total_bytes_committed,
            total_quality_adj_power,
            total_qa_bytes_committed,
            total_pledge_collateral,
            this_epoch_raw_byte_power,
            this_epoch_quality_adj_power,
            this_epoch_pledge_collateral,
            this_epoch_qa_power_smoothed,
            miner_count,
            miner_above_min_power_count,
            cron_event_queue,
            first_cron_epoch,
            claims,
            proof_validation_batch,
            ramp_start_epoch,
            ramp_duration_epochs,
        })
    }

    /// Consume state to return just total quality adj power
    pub fn into_total_quality_adj_power(self) -> StoragePower {
        delegate_state!(self.total_quality_adj_power)
    }

    /// Returns the addresses of every miner that has claimed power in the power actor
    pub fn list_all_miners<BS: Blockstore>(self, store: &BS) -> anyhow::Result<Vec<Address>> {
        match self {
            State::V8(st) => list_miners_for_state!(st, store, v8),
            State::V9(st) => list_miners_for_state!(st, store, v9),
            State::V10(st) => list_miners_for_state!(st, store, v10),
            State::V11(st) => list_miners_for_state!(st, store, v11),
            State::V12(st) => {
                let claims = st.load_claims(store)?;
                let mut miners = Vec::new();
                claims.for_each(|addr, _claim| {
                    miners.push(addr.into());
                    Ok(())
                })?;
                Ok(miners)
            }
            State::V13(st) => {
                let claims = st.load_claims(store)?;
                let mut miners = Vec::new();
                claims.for_each(|addr, _claim| {
                    miners.push(addr.into());
                    Ok(())
                })?;
                Ok(miners)
            }
            State::V14(st) => {
                let claims = st.load_claims(store)?;
                let mut miners = Vec::new();
                claims.for_each(|addr, _claim| {
                    miners.push(addr.into());
                    Ok(())
                })?;
                Ok(miners)
            }
            State::V15(st) => {
                let claims = st.load_claims(store)?;
                let mut miners = Vec::new();
                claims.for_each(|addr, _claim| {
                    miners.push(addr.into());
                    Ok(())
                })?;
                Ok(miners)
            }
            State::V16(st) => {
                let claims = st.load_claims(store)?;
                let mut miners = Vec::new();
                claims.for_each(|addr, _claim| {
                    miners.push(addr.into());
                    Ok(())
                })?;
                Ok(miners)
            }
            State::V17(st) => {
                let claims = st.load_claims(store)?;
                let mut miners = Vec::new();
                claims.for_each(|addr, _claim| {
                    miners.push(addr.into());
                    Ok(())
                })?;
                Ok(miners)
            }
        }
    }

    /// Returns the total power claim.
    pub fn total_power(&self) -> Claim {
        delegate_state!(self => |st| Claim {
            raw_byte_power: st.total_raw_byte_power.clone(),
            quality_adj_power: st.total_quality_adj_power.clone(),
        })
    }

    /// Consume state to return total locked funds
    pub fn into_total_locked(self) -> TokenAmount {
        delegate_state!(self.into_total_locked().into())
    }

    /// Loads power for a given miner, if exists.
    pub fn miner_power<BS: Blockstore>(
        &self,
        s: &BS,
        miner: &Address,
    ) -> anyhow::Result<Option<Claim>> {
        delegate_state!(self => |st| Ok(st.miner_power(&s, &miner.into())?.map(From::from)))
    }

    /// Checks power actor state for if miner meets minimum consensus power.
    pub fn miner_nominal_power_meets_consensus_minimum<BS: Blockstore>(
        &self,
        policy: &Policy,
        s: &BS,
        miner: &Address,
    ) -> anyhow::Result<bool> {
        match self {
            State::V8(st) => {
                st.miner_nominal_power_meets_consensus_minimum(&policy.into(), &s, &miner.into())
            }
            State::V9(st) => {
                st.miner_nominal_power_meets_consensus_minimum(&policy.into(), &s, &miner.into())
            }
            State::V10(st) => st
                .miner_nominal_power_meets_consensus_minimum(&policy.into(), &s, miner.id()?)
                .map(|(_, bool_val)| bool_val)
                .map_err(|e| anyhow::anyhow!("{}", e)),
            State::V11(st) => st
                .miner_nominal_power_meets_consensus_minimum(&policy.into(), &s, miner.id()?)
                .map(|(_, bool_val)| bool_val)
                .map_err(|e| anyhow::anyhow!("{}", e)),
            State::V12(st) => st
                .miner_nominal_power_meets_consensus_minimum(&policy.into(), &s, miner.id()?)
                .map(|(_, bool_val)| bool_val)
                .map_err(|e| anyhow::anyhow!("{}", e)),
            State::V13(st) => st
                .miner_nominal_power_meets_consensus_minimum(&policy.0, &s, miner.id()?)
                .map(|(_, bool_val)| bool_val)
                .map_err(|e| anyhow::anyhow!("{}", e)),
            State::V14(st) => st
                .miner_nominal_power_meets_consensus_minimum(&policy.into(), &s, miner.id()?)
                .map(|(_, bool_val)| bool_val)
                .map_err(|e| anyhow::anyhow!("{}", e)),
            State::V15(st) => st
                .miner_nominal_power_meets_consensus_minimum(&policy.into(), &s, miner.id()?)
                .map(|(_, bool_val)| bool_val)
                .map_err(|e| anyhow::anyhow!("{}", e)),
            State::V16(st) => st
                .miner_nominal_power_meets_consensus_minimum(&policy.into(), &s, miner.id()?)
                .map(|(_, bool_val)| bool_val)
                .map_err(|e| anyhow::anyhow!("{}", e)),
            State::V17(st) => st
                .miner_nominal_power_meets_consensus_minimum(&policy.into(), &s, miner.id()?)
                .map(|(_, bool_val)| bool_val)
                .map_err(|e| anyhow::anyhow!("{}", e)),
        }
    }

    /// Returns `this_epoch_qa_power_smoothed` from the state.
    pub fn total_power_smoothed(&self) -> FilterEstimate {
        match self {
            State::V8(st) => st.this_epoch_qa_power_smoothed.clone(),
            State::V9(st) => st.this_epoch_qa_power_smoothed.clone(),
            State::V10(st) => {
                from_filter_estimate_v3_to_v2(st.this_epoch_qa_power_smoothed.clone())
            }
            State::V11(st) => {
                from_filter_estimate_v3_to_v2(st.this_epoch_qa_power_smoothed.clone())
            }
            State::V12(st) => {
                from_filter_estimate_v3_to_v2(st.this_epoch_qa_power_smoothed.clone())
            }
            State::V13(st) => {
                from_filter_estimate_v3_to_v2(st.this_epoch_qa_power_smoothed.clone())
            }
            State::V14(st) => FilterEstimate {
                position: st.this_epoch_qa_power_smoothed.clone().position,
                velocity: st.this_epoch_qa_power_smoothed.clone().velocity,
            },
            State::V15(st) => FilterEstimate {
                position: st.this_epoch_qa_power_smoothed.clone().position,
                velocity: st.this_epoch_qa_power_smoothed.clone().velocity,
            },
            State::V16(st) => FilterEstimate {
                position: st.this_epoch_qa_power_smoothed.clone().position,
                velocity: st.this_epoch_qa_power_smoothed.clone().velocity,
            },
            State::V17(st) => FilterEstimate {
                position: st.this_epoch_qa_power_smoothed.clone().position,
                velocity: st.this_epoch_qa_power_smoothed.clone().velocity,
            },
        }
    }

    /// Returns total locked funds
    pub fn total_locked(&self) -> TokenAmount {
        delegate_state!(self.total_pledge_collateral.clone().into())
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct Claim {
    /// Sum of raw byte power for a miner's sectors.
    pub raw_byte_power: StoragePower,
    /// Sum of quality adjusted power for a miner's sectors.
    pub quality_adj_power: StoragePower,
}

impl From<fil_actor_power_state::v8::Claim> for Claim {
    fn from(cl: fil_actor_power_state::v8::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}

impl From<fil_actor_power_state::v9::Claim> for Claim {
    fn from(cl: fil_actor_power_state::v9::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}

impl From<fil_actor_power_state::v10::Claim> for Claim {
    fn from(cl: fil_actor_power_state::v10::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}

impl From<fil_actor_power_state::v11::Claim> for Claim {
    fn from(cl: fil_actor_power_state::v11::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}

impl From<fil_actor_power_state::v12::Claim> for Claim {
    fn from(cl: fil_actor_power_state::v12::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}

impl From<fil_actor_power_state::v13::Claim> for Claim {
    fn from(cl: fil_actor_power_state::v13::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}

impl From<fil_actor_power_state::v14::Claim> for Claim {
    fn from(cl: fil_actor_power_state::v14::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}

impl From<fil_actor_power_state::v15::Claim> for Claim {
    fn from(cl: fil_actor_power_state::v15::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}

impl From<fil_actor_power_state::v16::Claim> for Claim {
    fn from(cl: fil_actor_power_state::v16::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}

impl From<fil_actor_power_state::v17::Claim> for Claim {
    fn from(cl: fil_actor_power_state::v17::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}

#[macro_export]
macro_rules! list_miners_for_state {
    ($state:ident, $store:ident, $version:ident) => {{
        let claims =
            fil_actors_shared::$version::make_map_with_root::<_, Claim>(&$state.claims, $store)?;
        let mut miners = Vec::new();
        claims.for_each(|bytes, _claim| {
            miners.push(Address::from_bytes(bytes).expect("Cannot get address from bytes"));
            Ok(())
        })?;
        Ok(miners)
    }};
}
