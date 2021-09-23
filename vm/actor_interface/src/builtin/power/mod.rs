// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::FilterEstimate;
use address::Address;
use fil_types::StoragePower;
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser;
use serde::Serialize;
use std::error::Error;
use vm::{ActorState, TokenAmount};

/// Power actor address.
/// TODO: Select based on actors version
pub static ADDRESS: &actorv4::STORAGE_POWER_ACTOR_ADDR = &actorv4::STORAGE_POWER_ACTOR_ADDR;

/// Power actor method.
/// TODO: Select based on actor version
pub type Method = actorv4::power::Method;

/// Power actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V0(actorv0::power::State),
    V2(actorv2::power::State),
    V3(actorv3::power::State),
    V4(actorv4::power::State),
    V5(actorv5::power::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> Result<State, Box<dyn Error>>
    where
        BS: BlockStore,
    {
        if actor.code == *actorv0::POWER_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V0)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv2::POWER_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V2)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv3::POWER_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V3)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv4::POWER_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V4)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv5::POWER_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V5)
                .ok_or("Actor state doesn't exist in store")?)
        } else {
            Err(format!("Unknown actor code {}", actor.code).into())
        }
    }

    /// Consume state to return just total quality adj power
    pub fn into_total_quality_adj_power(self) -> StoragePower {
        match self {
            State::V0(st) => st.total_quality_adj_power,
            State::V2(st) => st.total_quality_adj_power,
            State::V3(st) => st.total_quality_adj_power,
            State::V4(st) => st.total_quality_adj_power,
            State::V5(st) => st.total_quality_adj_power,
        }
    }

    /// Returns the total power claim.
    pub fn total_power(&self) -> Claim {
        match self {
            State::V0(st) => Claim {
                raw_byte_power: st.total_raw_byte_power.clone(),
                quality_adj_power: st.total_quality_adj_power.clone(),
            },
            State::V2(st) => Claim {
                raw_byte_power: st.total_raw_byte_power.clone(),
                quality_adj_power: st.total_quality_adj_power.clone(),
            },
            State::V3(st) => Claim {
                raw_byte_power: st.total_raw_byte_power.clone(),
                quality_adj_power: st.total_quality_adj_power.clone(),
            },
            State::V4(st) => Claim {
                raw_byte_power: st.total_raw_byte_power.clone(),
                quality_adj_power: st.total_quality_adj_power.clone(),
            },
            State::V5(st) => Claim {
                raw_byte_power: st.total_raw_byte_power.clone(),
                quality_adj_power: st.total_quality_adj_power.clone(),
            },
        }
    }

    /// Consume state to return total locked funds
    pub fn into_total_locked(self) -> TokenAmount {
        match self {
            State::V0(st) => st.into_total_locked(),
            State::V2(st) => st.into_total_locked(),
            State::V3(st) => st.into_total_locked(),
            State::V4(st) => st.into_total_locked(),
            State::V5(st) => st.into_total_locked(),
        }
    }

    /// Loads power for a given miner, if exists.
    pub fn miner_power<BS: BlockStore>(
        &self,
        s: &BS,
        miner: &Address,
    ) -> Result<Option<Claim>, Box<dyn Error>> {
        match self {
            State::V0(st) => Ok(st.miner_power(s, miner)?.map(From::from)),
            State::V2(st) => Ok(st.miner_power(s, miner)?.map(From::from)),
            State::V3(st) => Ok(st.miner_power(s, miner)?.map(From::from)),
            State::V4(st) => Ok(st.miner_power(s, miner)?.map(From::from)),
            State::V5(st) => Ok(st.miner_power(s, miner)?.map(From::from)),
        }
    }

    /// Loads power for a given miner, if exists.
    pub fn list_all_miners<BS: BlockStore>(&self, s: &BS) -> Result<Vec<Address>, Box<dyn Error>> {
        match self {
            State::V0(st) => {
                let claims = actorv0::make_map_with_root(&st.claims, s)?;
                let mut miners = Vec::new();
                claims.for_each(|k, _: &actorv0::power::Claim| {
                    miners.push(Address::from_bytes(&k.0)?);
                    Ok(())
                })?;

                Ok(miners)
            }
            State::V2(st) => {
                let claims = actorv2::make_map_with_root(&st.claims, s)?;
                let mut miners = Vec::new();
                claims.for_each(|k, _: &actorv2::power::Claim| {
                    miners.push(Address::from_bytes(&k.0)?);
                    Ok(())
                })?;

                Ok(miners)
            }
            State::V3(st) => {
                let claims = actorv3::make_map_with_root(&st.claims, s)?;
                let mut miners = Vec::new();
                claims.for_each(|k, _: &actorv3::power::Claim| {
                    miners.push(Address::from_bytes(&k.0)?);
                    Ok(())
                })?;

                Ok(miners)
            }
            State::V4(st) => {
                let claims = actorv4::make_map_with_root(&st.claims, s)?;
                let mut miners = Vec::new();
                claims.for_each(|k, _: &actorv3::power::Claim| {
                    miners.push(Address::from_bytes(&k.0)?);
                    Ok(())
                })?;

                Ok(miners)
            }
            State::V5(st) => {
                let claims = actorv5::make_map_with_root(&st.claims, s)?;
                let mut miners = Vec::new();
                claims.for_each(|k, _: &actorv3::power::Claim| {
                    miners.push(Address::from_bytes(&k.0)?);
                    Ok(())
                })?;

                Ok(miners)
            }
        }
    }

    /// Checks power actor state for if miner meets minimum consensus power.
    pub fn miner_nominal_power_meets_consensus_minimum<BS: BlockStore>(
        &self,
        s: &BS,
        miner: &Address,
    ) -> Result<bool, Box<dyn Error>> {
        match self {
            State::V0(st) => st.miner_nominal_power_meets_consensus_minimum(s, miner),
            State::V2(st) => st.miner_nominal_power_meets_consensus_minimum(s, miner),
            State::V3(st) => st.miner_nominal_power_meets_consensus_minimum(s, miner),
            State::V4(st) => st.miner_nominal_power_meets_consensus_minimum(s, miner),
            State::V5(st) => st.miner_nominal_power_meets_consensus_minimum(s, miner),
        }
    }

    /// Returns this_epoch_qa_power_smoothed from the state.
    pub fn total_power_smoothed(&self) -> FilterEstimate {
        match self {
            State::V0(st) => st.this_epoch_qa_power_smoothed.clone().into(),
            State::V2(st) => st.this_epoch_qa_power_smoothed.clone().into(),
            State::V3(st) => st.this_epoch_qa_power_smoothed.clone().into(),
            State::V4(st) => st.this_epoch_qa_power_smoothed.clone().into(),
            State::V5(st) => st.this_epoch_qa_power_smoothed.clone().into(),
        }
    }

    /// Returns total locked funds
    pub fn total_locked(&self) -> TokenAmount {
        match self {
            State::V0(st) => st.total_pledge_collateral.clone(),
            State::V2(st) => st.total_pledge_collateral.clone(),
            State::V3(st) => st.total_pledge_collateral.clone(),
            State::V4(st) => st.total_pledge_collateral.clone(),
            State::V5(st) => st.total_pledge_collateral.clone(),
        }
    }
}

#[derive(Default, Debug, Serialize, Clone)]
pub struct Claim {
    /// Sum of raw byte power for a miner's sectors.
    #[serde(with = "bigint_ser::json")]
    pub raw_byte_power: StoragePower,
    /// Sum of quality adjusted power for a miner's sectors.
    #[serde(with = "bigint_ser::json")]
    pub quality_adj_power: StoragePower,
}

impl From<actorv0::power::Claim> for Claim {
    fn from(cl: actorv0::power::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}

impl From<actorv2::power::Claim> for Claim {
    fn from(cl: actorv2::power::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}

impl From<actorv3::power::Claim> for Claim {
    fn from(cl: actorv3::power::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}

impl From<actorv4::power::Claim> for Claim {
    fn from(cl: actorv4::power::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}

impl From<actorv5::power::Claim> for Claim {
    fn from(cl: actorv5::power::Claim) -> Self {
        Self {
            raw_byte_power: cl.raw_byte_power,
            quality_adj_power: cl.quality_adj_power,
        }
    }
}
