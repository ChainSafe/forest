// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod ext;

use crate::list_miners_for_state;
use crate::shim::actors::FilterEstimate;
use crate::shim::actors::Policy;
use crate::shim::actors::convert::*;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared2::{address::Address, econ::TokenAmount, sector::StoragePower};
use serde::{Deserialize, Serialize};

/// Power actor address.
// TODO(forest): https://github.com/ChainSafe/forest/issues/5011
pub const ADDRESS: Address = Address::new_id(4);

/// Power actor method.
// TODO(forest): https://github.com/ChainSafe/forest/issues/5011
pub type Method = fil_actor_power_state::v8::Method;

/// Power actor state.
#[derive(Serialize, Debug)]
#[serde(untagged)]
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
}

impl State {
    /// Consume state to return just total quality adj power
    pub fn into_total_quality_adj_power(self) -> StoragePower {
        match self {
            State::V8(st) => st.total_quality_adj_power,
            State::V9(st) => st.total_quality_adj_power,
            State::V10(st) => st.total_quality_adj_power,
            State::V11(st) => st.total_quality_adj_power,
            State::V12(st) => st.total_quality_adj_power,
            State::V13(st) => st.total_quality_adj_power,
            State::V14(st) => st.total_quality_adj_power,
            State::V15(st) => st.total_quality_adj_power,
            State::V16(st) => st.total_quality_adj_power,
        }
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
                    miners.push(from_address_v4_to_v2(addr));
                    Ok(())
                })?;
                Ok(miners)
            }
            State::V13(st) => {
                let claims = st.load_claims(store)?;
                let mut miners = Vec::new();
                claims.for_each(|addr, _claim| {
                    miners.push(from_address_v4_to_v2(addr));
                    Ok(())
                })?;
                Ok(miners)
            }
            State::V14(st) => {
                let claims = st.load_claims(store)?;
                let mut miners = Vec::new();
                claims.for_each(|addr, _claim| {
                    miners.push(from_address_v4_to_v2(addr));
                    Ok(())
                })?;
                Ok(miners)
            }
            State::V15(st) => {
                let claims = st.load_claims(store)?;
                let mut miners = Vec::new();
                claims.for_each(|addr, _claim| {
                    miners.push(from_address_v4_to_v2(addr));
                    Ok(())
                })?;
                Ok(miners)
            }
            State::V16(st) => {
                let claims = st.load_claims(store)?;
                let mut miners = Vec::new();
                claims.for_each(|addr, _claim| {
                    miners.push(from_address_v4_to_v2(addr));
                    Ok(())
                })?;
                Ok(miners)
            }
        }
    }

    /// Returns the total power claim.
    pub fn total_power(&self) -> Claim {
        match self {
            State::V8(st) => Claim {
                raw_byte_power: st.total_raw_byte_power.clone(),
                quality_adj_power: st.total_quality_adj_power.clone(),
            },
            State::V9(st) => Claim {
                raw_byte_power: st.total_raw_byte_power.clone(),
                quality_adj_power: st.total_quality_adj_power.clone(),
            },
            State::V10(st) => Claim {
                raw_byte_power: st.total_raw_byte_power.clone(),
                quality_adj_power: st.total_quality_adj_power.clone(),
            },
            State::V11(st) => Claim {
                raw_byte_power: st.total_raw_byte_power.clone(),
                quality_adj_power: st.total_quality_adj_power.clone(),
            },
            State::V12(st) => Claim {
                raw_byte_power: st.total_raw_byte_power.clone(),
                quality_adj_power: st.total_quality_adj_power.clone(),
            },
            State::V13(st) => Claim {
                raw_byte_power: st.total_raw_byte_power.clone(),
                quality_adj_power: st.total_quality_adj_power.clone(),
            },
            State::V14(st) => Claim {
                raw_byte_power: st.total_raw_byte_power.clone(),
                quality_adj_power: st.total_quality_adj_power.clone(),
            },
            State::V15(st) => Claim {
                raw_byte_power: st.total_raw_byte_power.clone(),
                quality_adj_power: st.total_quality_adj_power.clone(),
            },
            State::V16(st) => Claim {
                raw_byte_power: st.total_raw_byte_power.clone(),
                quality_adj_power: st.total_quality_adj_power.clone(),
            },
        }
    }

    /// Consume state to return total locked funds
    pub fn into_total_locked(self) -> TokenAmount {
        match self {
            State::V8(st) => st.into_total_locked(),
            State::V9(st) => st.into_total_locked(),
            State::V10(st) => from_token_v3_to_v2(&st.into_total_locked()),
            State::V11(st) => from_token_v3_to_v2(&st.into_total_locked()),
            State::V12(st) => from_token_v4_to_v2(&st.into_total_locked()),
            State::V13(st) => from_token_v4_to_v2(&st.into_total_locked()),
            State::V14(st) => from_token_v4_to_v2(&st.into_total_locked()),
            State::V15(st) => from_token_v4_to_v2(&st.into_total_locked()),
            State::V16(st) => from_token_v4_to_v2(&st.into_total_locked()),
        }
    }

    /// Loads power for a given miner, if exists.
    pub fn miner_power<BS: Blockstore>(
        &self,
        s: &BS,
        miner: &Address,
    ) -> anyhow::Result<Option<Claim>> {
        match self {
            State::V8(st) => Ok(st.miner_power(&s, miner)?.map(From::from)),
            State::V9(st) => Ok(st.miner_power(&s, miner)?.map(From::from)),
            State::V10(st) => Ok(st
                .miner_power(&s, &from_address_v2_to_v3(*miner))?
                .map(From::from)),
            State::V11(st) => Ok(st
                .miner_power(&s, &from_address_v2_to_v3(*miner))?
                .map(From::from)),
            State::V12(st) => Ok(st
                .miner_power(&s, &from_address_v2_to_v4(*miner))?
                .map(From::from)),
            State::V13(st) => Ok(st
                .miner_power(&s, &from_address_v2_to_v4(*miner))?
                .map(From::from)),
            State::V14(st) => Ok(st
                .miner_power(&s, &from_address_v2_to_v4(*miner))?
                .map(From::from)),
            State::V15(st) => Ok(st
                .miner_power(&s, &from_address_v2_to_v4(*miner))?
                .map(From::from)),
            State::V16(st) => Ok(st
                .miner_power(&s, &from_address_v2_to_v4(*miner))?
                .map(From::from)),
        }
    }

    /// Checks power actor state for if miner meets minimum consensus power.
    pub fn miner_nominal_power_meets_consensus_minimum<BS: Blockstore>(
        &self,
        policy: &Policy,
        s: &BS,
        miner: &Address,
    ) -> anyhow::Result<bool> {
        match self {
            State::V8(st) => st.miner_nominal_power_meets_consensus_minimum(
                &from_policy_v13_to_v9(policy),
                &s,
                miner,
            ),
            State::V9(st) => st.miner_nominal_power_meets_consensus_minimum(
                &from_policy_v13_to_v9(policy),
                &s,
                miner,
            ),
            State::V10(st) => st
                .miner_nominal_power_meets_consensus_minimum(
                    &from_policy_v13_to_v10(policy),
                    &s,
                    miner.id()?,
                )
                .map(|(_, bool_val)| bool_val)
                .map_err(|e| anyhow::anyhow!("{}", e)),
            State::V11(st) => st
                .miner_nominal_power_meets_consensus_minimum(
                    &from_policy_v13_to_v11(policy),
                    &s,
                    miner.id()?,
                )
                .map(|(_, bool_val)| bool_val)
                .map_err(|e| anyhow::anyhow!("{}", e)),
            State::V12(st) => st
                .miner_nominal_power_meets_consensus_minimum(
                    &from_policy_v13_to_v12(policy),
                    &s,
                    miner.id()?,
                )
                .map(|(_, bool_val)| bool_val)
                .map_err(|e| anyhow::anyhow!("{}", e)),
            State::V13(st) => st
                .miner_nominal_power_meets_consensus_minimum(policy, &s, miner.id()?)
                .map(|(_, bool_val)| bool_val)
                .map_err(|e| anyhow::anyhow!("{}", e)),
            State::V14(st) => st
                .miner_nominal_power_meets_consensus_minimum(
                    &from_policy_v13_to_v14(policy),
                    &s,
                    miner.id()?,
                )
                .map(|(_, bool_val)| bool_val)
                .map_err(|e| anyhow::anyhow!("{}", e)),
            State::V15(st) => st
                .miner_nominal_power_meets_consensus_minimum(
                    &from_policy_v13_to_v15(policy),
                    &s,
                    miner.id()?,
                )
                .map(|(_, bool_val)| bool_val)
                .map_err(|e| anyhow::anyhow!("{}", e)),
            State::V16(st) => st
                .miner_nominal_power_meets_consensus_minimum(
                    &from_policy_v13_to_v16(policy),
                    &s,
                    miner.id()?,
                )
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
                from_filter_estimate_v4_to_v2(st.this_epoch_qa_power_smoothed.clone())
            }
            State::V13(st) => {
                from_filter_estimate_v4_to_v2(st.this_epoch_qa_power_smoothed.clone())
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
        }
    }

    /// Returns total locked funds
    pub fn total_locked(&self) -> TokenAmount {
        match self {
            State::V8(st) => st.total_pledge_collateral.clone(),
            State::V9(st) => st.total_pledge_collateral.clone(),
            State::V10(st) => from_token_v3_to_v2(&st.total_pledge_collateral),
            State::V11(st) => from_token_v3_to_v2(&st.total_pledge_collateral),
            State::V12(st) => from_token_v4_to_v2(&st.total_pledge_collateral.clone()),
            State::V13(st) => from_token_v4_to_v2(&st.total_pledge_collateral.clone()),
            State::V14(st) => from_token_v4_to_v2(&st.total_pledge_collateral.clone()),
            State::V15(st) => from_token_v4_to_v2(&st.total_pledge_collateral.clone()),
            State::V16(st) => from_token_v4_to_v2(&st.total_pledge_collateral.clone()),
        }
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
