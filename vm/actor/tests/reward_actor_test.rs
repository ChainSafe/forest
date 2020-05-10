// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod common;

use actor::{
    reward::{AwardBlockRewardParams, Method},
    REWARD_ACTOR_ADDR, REWARD_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR, SYSTEM_ACTOR_CODE_ID,
};
use address::Address;
use common::*;
use db::MemoryDB;
use ipld_blockstore::BlockStore;
use message::UnsignedMessage;
use vm::{ExitCode, Serialized, TokenAmount, METHOD_CONSTRUCTOR};

fn construct_runtime<BS: BlockStore>(bs: &BS) -> MockRuntime<'_, BS> {
    let message = UnsignedMessage::builder()
        .to(*REWARD_ACTOR_ADDR)
        .from(*SYSTEM_ACTOR_ADDR)
        .build()
        .unwrap();
    let mut rt = MockRuntime::new(bs, message);
    rt.caller_type = SYSTEM_ACTOR_CODE_ID.clone();
    return rt;
}

#[test]
fn balance_less_than_reward() {
    let bs = MemoryDB::default();
    let mut rt = construct_runtime(&bs);
    construct_and_verify(&mut rt);

    let miner = Address::new_id(1000);
    let gas_reward = TokenAmount::from(10u8);

    rt.expect_validate_caller_addr(&[*SYSTEM_ACTOR_ADDR]);

    let params = AwardBlockRewardParams {
        miner: miner,
        penalty: TokenAmount::from(0u8),
        gas_reward: gas_reward,
        ticket_count: 0,
    };

    //Expect call to fail because actor doesnt have enough tokens to reward
    let call_result = rt.call(
        &*REWARD_ACTOR_CODE_ID,
        Method::AwardBlockReward as u64,
        &Serialized::serialize(&params).unwrap(),
    );

    assert_eq!(
        ExitCode::ErrInsufficientFunds,
        call_result.unwrap_err().exit_code()
    );

    rt.verify()
}

fn construct_and_verify<BS: BlockStore>(rt: &mut MockRuntime<'_, BS>) {
    rt.expect_validate_caller_addr(&[SYSTEM_ACTOR_ADDR.clone()]);
    let ret = rt
        .call(
            &*REWARD_ACTOR_CODE_ID,
            METHOD_CONSTRUCTOR,
            &Serialized::default(),
        )
        .unwrap();

    assert_eq!(Serialized::default(), ret);
    rt.verify();
}
