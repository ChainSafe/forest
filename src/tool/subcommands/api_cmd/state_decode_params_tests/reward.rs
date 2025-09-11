// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use fil_actor_reward_state::v17::*;
use num_bigint::BigInt;

/// Creates state decode params tests for the Reward actor.
pub fn create_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    let reward_constructor_params = ConstructorParams {
        power: Some(Default::default()),
    };

    let reward_award_block_reward_params = AwardBlockRewardParams {
        miner: Address::new_id(1000).into(),
        penalty: Default::default(),
        gas_reward: Default::default(),
        win_count: 0,
    };

    let reward_update_network_params = UpdateNetworkKPIParams {
        curr_realized_power: Option::from(fvm_shared4::bigint::bigint_ser::BigIntDe(BigInt::from(
            111,
        ))),
    };

    Ok(vec![
        RpcTest::identity(StateDecodeParams::request((
            Address::REWARD_ACTOR,
            Method::Constructor as u64,
            to_vec(&reward_constructor_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::REWARD_ACTOR,
            Method::AwardBlockReward as u64,
            to_vec(&reward_award_block_reward_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::REWARD_ACTOR,
            Method::UpdateNetworkKPI as u64,
            to_vec(&reward_update_network_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            Address::REWARD_ACTOR,
            Method::ThisEpochReward as u64,
            vec![],
            tipset.key().into(),
        ))?),
    ])
}
