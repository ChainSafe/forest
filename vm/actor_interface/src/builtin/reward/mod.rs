// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::FilterEstimate;
use cid::multihash::MultihashDigest;
use fil_types::StoragePower;
use ipld_blockstore::BlockStore;
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
    // V0(actorv0::reward::State),
    // V2(actorv2::reward::State),
    // V3(actorv3::reward::State),
    // V4(actorv4::reward::State),
    // V5(actorv5::reward::State),
    // V6(actorv6::reward::State),
    V7(fil_actor_reward_v7::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        if actor.code == cid::Cid::new_v1(cid::RAW, cid::Code::Identity.digest(b"fil/7/reward")) {
            Ok(store
                .get_anyhow(&actor.state)?
                .map(State::V7)
                .context("Actor state doesn't exist in store")?)
        } else {
            Err(anyhow::anyhow!("Unknown reward actor code {}", actor.code))
        }
    }

    /// Consume state to return just storage power reward
    pub fn into_total_storage_power_reward(self) -> StoragePower {
        match self {
            // State::V0(st) => st.into_total_storage_power_reward(),
            // State::V2(st) => st.into_total_storage_power_reward(),
            // State::V3(st) => st.into_total_storage_power_reward(),
            // State::V4(st) => st.into_total_storage_power_reward(),
            // State::V5(st) => st.into_total_storage_power_reward(),
            // State::V6(st) => st.into_total_storage_power_reward(),
            State::V7(st) => st.into_total_storage_power_reward(),
        }
    }

    pub fn pre_commit_deposit_for_power(
        &self,
        _network_qa_power: FilterEstimate,
        _sector_weight: &StoragePower,
    ) -> TokenAmount {
        match self {
            // State::V0(st) => actorv0::miner::pre_commit_deposit_for_power(
            //     &st.this_epoch_reward_smoothed,
            //     &actorv0::util::smooth::FilterEstimate {
            //         position: network_qa_power.position,
            //         velocity: network_qa_power.velocity,
            //     },
            //     sector_weight,
            // ),
            // State::V2(st) => actorv2::miner::pre_commit_deposit_for_power(
            //     &st.this_epoch_reward_smoothed,
            //     &actorv2::util::smooth::FilterEstimate {
            //         position: network_qa_power.position,
            //         velocity: network_qa_power.velocity,
            //     },
            //     sector_weight,
            // ),
            // State::V3(st) => actorv3::miner::pre_commit_deposit_for_power(
            //     &st.this_epoch_reward_smoothed,
            //     &actorv3::util::smooth::FilterEstimate {
            //         position: network_qa_power.position,
            //         velocity: network_qa_power.velocity,
            //     },
            //     sector_weight,
            // ),
            // State::V4(st) => actorv4::miner::pre_commit_deposit_for_power(
            //     &st.this_epoch_reward_smoothed,
            //     &actorv4::util::smooth::FilterEstimate {
            //         position: network_qa_power.position,
            //         velocity: network_qa_power.velocity,
            //     },
            //     sector_weight,
            // ),
            // State::V5(st) => actorv5::miner::pre_commit_deposit_for_power(
            //     &st.this_epoch_reward_smoothed,
            //     &actorv5::util::smooth::FilterEstimate {
            //         position: network_qa_power.position,
            //         velocity: network_qa_power.velocity,
            //     },
            //     sector_weight,
            // ),
            // State::V6(st) => actorv6::miner::pre_commit_deposit_for_power(
            //     &st.this_epoch_reward_smoothed,
            //     &actorv6::util::smooth::FilterEstimate {
            //         position: network_qa_power.position,
            //         velocity: network_qa_power.velocity,
            //     },
            //     sector_weight,
            // ),
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
            // State::V0(st) => actorv0::miner::initial_pledge_for_power(
            //     sector_weight,
            //     &st.this_epoch_baseline_power,
            //     &st.this_epoch_reward_smoothed,
            //     &actorv0::util::smooth::FilterEstimate {
            //         position: network_qa_power.position,
            //         velocity: network_qa_power.velocity,
            //     },
            //     circ_supply,
            // ),
            // State::V2(st) => actorv2::miner::initial_pledge_for_power(
            //     sector_weight,
            //     &st.this_epoch_baseline_power,
            //     &st.this_epoch_reward_smoothed,
            //     &actorv2::util::smooth::FilterEstimate {
            //         position: network_qa_power.position,
            //         velocity: network_qa_power.velocity,
            //     },
            //     circ_supply,
            // ),
            // State::V3(st) => actorv3::miner::initial_pledge_for_power(
            //     sector_weight,
            //     &st.this_epoch_baseline_power,
            //     &st.this_epoch_reward_smoothed,
            //     &actorv3::util::smooth::FilterEstimate {
            //         position: network_qa_power.position,
            //         velocity: network_qa_power.velocity,
            //     },
            //     circ_supply,
            // ),
            // State::V4(st) => actorv4::miner::initial_pledge_for_power(
            //     sector_weight,
            //     &st.this_epoch_baseline_power,
            //     &st.this_epoch_reward_smoothed,
            //     &actorv4::util::smooth::FilterEstimate {
            //         position: network_qa_power.position,
            //         velocity: network_qa_power.velocity,
            //     },
            //     circ_supply,
            // ),
            // State::V5(st) => actorv5::miner::initial_pledge_for_power(
            //     sector_weight,
            //     &st.this_epoch_baseline_power,
            //     &st.this_epoch_reward_smoothed,
            //     &actorv5::util::smooth::FilterEstimate {
            //         position: network_qa_power.position,
            //         velocity: network_qa_power.velocity,
            //     },
            //     circ_supply,
            // ),
            // State::V6(st) => actorv6::miner::initial_pledge_for_power(
            //     sector_weight,
            //     &st.this_epoch_baseline_power,
            //     &st.this_epoch_reward_smoothed,
            //     &actorv6::util::smooth::FilterEstimate {
            //         position: network_qa_power.position,
            //         velocity: network_qa_power.velocity,
            //     },
            //     circ_supply,
            // ),
            State::V7(_st) => todo!(),
        }
    }
}
