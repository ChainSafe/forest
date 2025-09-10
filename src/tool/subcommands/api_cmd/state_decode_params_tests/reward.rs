// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use num_bigint::BigInt;

/// Creates state decode params tests for the Reward actor.
pub fn create_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    let reward_constructor_params = fil_actor_reward_state::v16::ConstructorParams {
        power: Some(Default::default()),
    };

    let reward_award_block_reward_params = fil_actor_reward_state::v16::AwardBlockRewardParams {
        miner: Address::new_id(1000).into(),
        penalty: Default::default(),
        gas_reward: Default::default(),
        win_count: 0,
    };

    let reward_update_network_params = fil_actor_reward_state::v16::UpdateNetworkKPIParams {
        curr_realized_power: Option::from(fvm_shared4::bigint::bigint_ser::BigIntDe(BigInt::from(
            111,
        ))),
    };

    Ok(vec![
        RpcTest::identity(StateDecodeParams::request((
            Address::REWARD_ACTOR,
            fil_actor_reward_state::v16::Method::Constructor as u64,
            to_vec(&reward_constructor_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::REWARD_ACTOR,
            fil_actor_reward_state::v16::Method::AwardBlockReward as u64,
            to_vec(&reward_award_block_reward_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::REWARD_ACTOR,
            fil_actor_reward_state::v16::Method::UpdateNetworkKPI as u64,
            to_vec(&reward_update_network_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::REWARD_ACTOR,
            fil_actor_reward_state::v16::Method::ThisEpochReward as u64,
            vec![],
            tipset.key().into(),
        ))?),
    ])
}
