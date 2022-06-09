// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{load_actor_state, FilterEstimate};
use fil_types::StoragePower;
use ipld_blockstore::BlockStore;
use serde::Serialize;
use vm::{ActorState, TokenAmount};

/// Reward actor address.
pub static ADDRESS: &actorv4::REWARD_ACTOR_ADDR = &actorv4::REWARD_ACTOR_ADDR;

/// Reward actor method.
pub type Method = actorv4::reward::Method;

/// Reward actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V0(actorv0::reward::State),
    V2(actorv2::reward::State),
    V3(actorv3::reward::State),
    V4(actorv4::reward::State),
    V5(actorv5::reward::State),
    V6(actorv6::reward::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        load_actor_state!(store, actor, REWARD_ACTOR_CODE_ID)
    }

    /// Consume state to return just storage power reward
    pub fn into_total_storage_power_reward(self) -> StoragePower {
        match self {
            State::V0(st) => st.into_total_storage_power_reward(),
            State::V2(st) => st.into_total_storage_power_reward(),
            State::V3(st) => st.into_total_storage_power_reward(),
            State::V4(st) => st.into_total_storage_power_reward(),
            State::V5(st) => st.into_total_storage_power_reward(),
            State::V6(st) => st.into_total_storage_power_reward(),
        }
    }

    pub fn pre_commit_deposit_for_power(
        &self,
        network_qa_power: FilterEstimate,
        sector_weight: &StoragePower,
    ) -> TokenAmount {
        match self {
            State::V0(st) => actorv0::miner::pre_commit_deposit_for_power(
                &st.this_epoch_reward_smoothed,
                &actorv0::util::smooth::FilterEstimate {
                    position: network_qa_power.position,
                    velocity: network_qa_power.velocity,
                },
                sector_weight,
            ),
            State::V2(st) => actorv2::miner::pre_commit_deposit_for_power(
                &st.this_epoch_reward_smoothed,
                &actorv2::util::smooth::FilterEstimate {
                    position: network_qa_power.position,
                    velocity: network_qa_power.velocity,
                },
                sector_weight,
            ),
            State::V3(st) => actorv3::miner::pre_commit_deposit_for_power(
                &st.this_epoch_reward_smoothed,
                &actorv3::util::smooth::FilterEstimate {
                    position: network_qa_power.position,
                    velocity: network_qa_power.velocity,
                },
                sector_weight,
            ),
            State::V4(st) => actorv4::miner::pre_commit_deposit_for_power(
                &st.this_epoch_reward_smoothed,
                &actorv4::util::smooth::FilterEstimate {
                    position: network_qa_power.position,
                    velocity: network_qa_power.velocity,
                },
                sector_weight,
            ),
            State::V5(st) => actorv5::miner::pre_commit_deposit_for_power(
                &st.this_epoch_reward_smoothed,
                &actorv5::util::smooth::FilterEstimate {
                    position: network_qa_power.position,
                    velocity: network_qa_power.velocity,
                },
                sector_weight,
            ),
            State::V6(st) => actorv6::miner::pre_commit_deposit_for_power(
                &st.this_epoch_reward_smoothed,
                &actorv6::util::smooth::FilterEstimate {
                    position: network_qa_power.position,
                    velocity: network_qa_power.velocity,
                },
                sector_weight,
            ),
        }
    }

    pub fn initial_pledge_for_power(
        &self,
        sector_weight: &StoragePower,
        _network_total_pledge: &TokenAmount,
        network_qa_power: FilterEstimate,
        circ_supply: &TokenAmount,
    ) -> TokenAmount {
        match self {
            State::V0(st) => actorv0::miner::initial_pledge_for_power(
                sector_weight,
                &st.this_epoch_baseline_power,
                &st.this_epoch_reward_smoothed,
                &actorv0::util::smooth::FilterEstimate {
                    position: network_qa_power.position,
                    velocity: network_qa_power.velocity,
                },
                circ_supply,
            ),
            State::V2(st) => actorv2::miner::initial_pledge_for_power(
                sector_weight,
                &st.this_epoch_baseline_power,
                &st.this_epoch_reward_smoothed,
                &actorv2::util::smooth::FilterEstimate {
                    position: network_qa_power.position,
                    velocity: network_qa_power.velocity,
                },
                circ_supply,
            ),
            State::V3(st) => actorv3::miner::initial_pledge_for_power(
                sector_weight,
                &st.this_epoch_baseline_power,
                &st.this_epoch_reward_smoothed,
                &actorv3::util::smooth::FilterEstimate {
                    position: network_qa_power.position,
                    velocity: network_qa_power.velocity,
                },
                circ_supply,
            ),
            State::V4(st) => actorv4::miner::initial_pledge_for_power(
                sector_weight,
                &st.this_epoch_baseline_power,
                &st.this_epoch_reward_smoothed,
                &actorv4::util::smooth::FilterEstimate {
                    position: network_qa_power.position,
                    velocity: network_qa_power.velocity,
                },
                circ_supply,
            ),
            State::V5(st) => actorv5::miner::initial_pledge_for_power(
                sector_weight,
                &st.this_epoch_baseline_power,
                &st.this_epoch_reward_smoothed,
                &actorv5::util::smooth::FilterEstimate {
                    position: network_qa_power.position,
                    velocity: network_qa_power.velocity,
                },
                circ_supply,
            ),
            State::V6(st) => actorv6::miner::initial_pledge_for_power(
                sector_weight,
                &st.this_epoch_baseline_power,
                &st.this_epoch_reward_smoothed,
                &actorv6::util::smooth::FilterEstimate {
                    position: network_qa_power.position,
                    velocity: network_qa_power.velocity,
                },
                circ_supply,
            ),
        }
    }
}
