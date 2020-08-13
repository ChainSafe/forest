// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod common;

use actor::{
    reward::{AwardBlockRewardParams, Method},
    REWARD_ACTOR_ADDR, REWARD_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR, SYSTEM_ACTOR_CODE_ID,
};
use address::Address;
use common::*;
use fil_types::StoragePower;
use num_bigint::bigint_ser::BigIntSer;
use vm::{Serialized, TokenAmount, METHOD_CONSTRUCTOR};

fn construct_runtime() -> MockRuntime {
    MockRuntime {
        receiver: *REWARD_ACTOR_ADDR,
        caller: *SYSTEM_ACTOR_ADDR,
        caller_type: SYSTEM_ACTOR_CODE_ID.clone(),
        ..Default::default()
    }
}

#[test]
// TODO update reward tests
#[ignore]
fn balance_less_than_reward() {
    let mut rt = construct_runtime();
    construct_and_verify(&mut rt, &Default::default());

    let miner = Address::new_id(1000);
    let gas_reward = TokenAmount::from(10u8);

    rt.expect_validate_caller_addr(vec![*SYSTEM_ACTOR_ADDR]);

    let params = AwardBlockRewardParams {
        miner: miner,
        penalty: TokenAmount::from(0u8),
        gas_reward: gas_reward,
        win_count: 0,
    };

    // Expect call to fail because actor doesnt have enough tokens to reward
    let _res = rt.call(
        &*REWARD_ACTOR_CODE_ID,
        Method::AwardBlockReward as u64,
        &Serialized::serialize(&params).unwrap(),
    );

    rt.verify()
}

fn construct_and_verify(rt: &mut MockRuntime, curr_power: &StoragePower) {
    rt.expect_validate_caller_addr(vec![SYSTEM_ACTOR_ADDR.clone()]);
    let ret = rt
        .call(
            &*REWARD_ACTOR_CODE_ID,
            METHOD_CONSTRUCTOR,
            &Serialized::serialize(BigIntSer(curr_power)).unwrap(),
        )
        .unwrap();

    assert_eq!(Serialized::default(), ret);
    rt.verify();
}
