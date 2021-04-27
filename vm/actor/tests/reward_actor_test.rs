// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;

mod common;

use address::Address;
use clock::ChainEpoch;
use common::*;
use fil_types::StoragePower;
use forest_actor::{
    miner::{ApplyRewardParams, Method as MinerMethod},
    reward::{
        AwardBlockRewardParams, Method, State, ThisEpochRewardReturn, BASELINE_INITIAL_VALUE,
        PENALTY_MULTIPLIER,
    },
    BURNT_FUNDS_ACTOR_ADDR, POWER_ACTOR_CODE_ID, REWARD_ACTOR_ADDR, REWARD_ACTOR_CODE_ID,
    STORAGE_POWER_ACTOR_ADDR, SYSTEM_ACTOR_ADDR, SYSTEM_ACTOR_CODE_ID,
};
use num_bigint::bigint_ser::BigIntSer;
use num_traits::FromPrimitive;
use vm::{ActorError, ExitCode, Serialized, TokenAmount, METHOD_CONSTRUCTOR, METHOD_SEND};

lazy_static! {
    static ref EPOCH_ZERO_REWARD: TokenAmount =
        TokenAmount::from_i128(36_266_264_293_777_134_739).unwrap();
    static ref WINNER: Address = Address::new_id(1000);
}

mod construction_tests {
    use super::*;
    #[test]
    fn construct_with_zero_power() {
        let start_realized_power = StoragePower::from(0);
        let rt = construct_and_verify(&start_realized_power);

        let state: State = rt.get_state().unwrap();

        assert_eq!(ChainEpoch::from(0), state.epoch);
        assert_eq!(start_realized_power, state.cumsum_realized);
        assert_eq!(*EPOCH_ZERO_REWARD, state.this_epoch_reward);
        assert_eq!(
            &*BASELINE_INITIAL_VALUE - 1,
            state.this_epoch_baseline_power
        );
        assert_eq!(&*BASELINE_INITIAL_VALUE, &state.effective_baseline_power);
    }

    #[test]
    fn construct_with_less_power_than_baseline() {
        let start_realized_power = StoragePower::from(1_i64 << 39);
        let rt = construct_and_verify(&start_realized_power);

        let state: State = rt.get_state().unwrap();
        assert_eq!(ChainEpoch::from(0), state.epoch);
        assert_eq!(start_realized_power, state.cumsum_realized);
        assert_ne!(TokenAmount::from(0), state.this_epoch_reward);
    }

    #[test]
    fn construct_with_more_power_than_baseline() {
        let mut start_realized_power = BASELINE_INITIAL_VALUE.clone();
        let rt = construct_and_verify(&start_realized_power);

        let state: State = rt.get_state().unwrap();
        let reward = state.this_epoch_reward;

        // start with 2x power
        start_realized_power *= 2;
        let rt = construct_and_verify(&start_realized_power);

        let state: State = rt.get_state().unwrap();
        assert_eq!(reward, state.this_epoch_reward);
    }
}

mod test_award_block_reward {
    use super::*;

    #[test]
    fn rejects_gas_reward_exceeding_balance() {
        let mut rt = construct_and_verify(&StoragePower::default());

        rt.set_balance(TokenAmount::from(9));
        rt.expect_validate_caller_addr(vec![*SYSTEM_ACTOR_ADDR]);
        assert_eq!(
            ExitCode::ErrIllegalState,
            award_block_reward(
                &mut rt,
                *WINNER,
                TokenAmount::from(0),
                TokenAmount::from(10),
                1,
                TokenAmount::from(0)
            )
            .unwrap_err()
            .exit_code()
        );
    }

    #[test]
    fn rejects_negative_penalty_or_reward() {
        let mut rt = construct_and_verify(&StoragePower::default());
        rt.set_balance(TokenAmount::from(10_i128.pow(18)));
        rt.expect_validate_caller_addr(vec![*SYSTEM_ACTOR_ADDR]);

        let reward_penalty_pairs = [(-1, 0), (0, -1)];

        for (reward, penalty) in &reward_penalty_pairs {
            assert_eq!(
                ExitCode::ErrIllegalArgument,
                award_block_reward(
                    &mut rt,
                    *WINNER,
                    TokenAmount::from(*penalty),
                    TokenAmount::from(*reward),
                    1,
                    TokenAmount::from(0)
                )
                .unwrap_err()
                .exit_code()
            );
            rt.reset();
        }
    }

    #[test]
    fn rejects_zero_wincount() {
        let mut rt = construct_and_verify(&StoragePower::default());
        rt.set_balance(TokenAmount::from(10_i128.pow(18)));

        rt.expect_validate_caller_addr(vec![*SYSTEM_ACTOR_ADDR]);
        assert!(award_block_reward(
            &mut rt,
            *WINNER,
            TokenAmount::from(0),
            TokenAmount::from(0),
            0,
            TokenAmount::from(0)
        )
        .is_err());
        rt.reset();
    }

    #[test]
    // TODO remove ignore when fixing (v0->v2 migration)
    #[ignore = "invalidated -- update"]
    fn pays_reward_and_burns_penalty() {
        let mut rt = construct_and_verify(&StoragePower::default());
        rt.set_balance(TokenAmount::from(10_i128.pow(27)));
        rt.expect_validate_caller_addr(vec![*SYSTEM_ACTOR_ADDR]);
        let penalty: TokenAmount = TokenAmount::from(100);
        let gas_reward: TokenAmount = TokenAmount::from(200);
        let expected_reward = &*EPOCH_ZERO_REWARD / 5 + &gas_reward - &penalty;
        assert!(
            award_block_reward(&mut rt, *WINNER, penalty, gas_reward, 1, expected_reward).is_ok()
        );
        rt.reset();
    }

    #[test]
    // TODO remove ignore when fixing (v0->v2 migration)
    #[ignore = "invalidated -- update"]
    fn pays_out_current_balance_when_reward_exceeds_total_balance() {
        let mut rt = construct_and_verify(&StoragePower::from(1));
        let small_reward = TokenAmount::from(300);
        rt.set_balance(small_reward.clone());
        rt.expect_validate_caller_addr(vec![*SYSTEM_ACTOR_ADDR]);

        let penalty = TokenAmount::from(100);
        let expected_reward = &small_reward - &penalty;

        rt.expect_send(
            *WINNER,
            MinerMethod::ApplyRewards as u64,
            Serialized::serialize(BigIntSer(&expected_reward)).unwrap(),
            expected_reward,
            Serialized::default(),
            ExitCode::Ok,
        );
        rt.expect_send(
            *BURNT_FUNDS_ACTOR_ADDR,
            METHOD_SEND,
            Serialized::default(),
            penalty.clone(),
            Serialized::default(),
            ExitCode::Ok,
        );

        let params = AwardBlockRewardParams {
            miner: *WINNER,
            penalty,
            gas_reward: TokenAmount::from(0),
            win_count: 1,
        };
        assert!(rt
            .call(
                &*REWARD_ACTOR_CODE_ID,
                Method::AwardBlockReward as u64,
                &Serialized::serialize(params).unwrap()
            )
            .is_ok());
        rt.verify();
    }

    #[test]
    fn total_mined_tracks_correctly() {
        let mut rt = construct_and_verify(&StoragePower::from(1));
        let mut state: State = rt.get_state().unwrap();

        assert_eq!(TokenAmount::from(0), state.total_storage_power_reward);
        state.this_epoch_reward = TokenAmount::from(5000);

        rt.replace_state(&state);

        let total_payout = TokenAmount::from(3500);
        rt.set_balance(total_payout.clone());

        for i in &[1000, 1000, 1000, 500] {
            assert!(award_block_reward(
                &mut rt,
                *WINNER,
                TokenAmount::from(0),
                TokenAmount::from(0),
                1,
                TokenAmount::from(*i)
            )
            .is_ok());
        }

        let new_state: State = rt.get_state().unwrap();
        assert_eq!(total_payout, new_state.total_storage_power_reward);
    }

    #[test]
    // TODO remove ignore when fixing (v0->v2 migration)
    #[ignore = "invalidated -- update"]
    fn funds_are_sent_to_burnt_funds_actor_if_sending_locked_funds_to_miner_fails() {
        let mut rt = construct_and_verify(&StoragePower::from(1));
        let mut state: State = rt.get_state().unwrap();

        assert_eq!(TokenAmount::from(0), state.total_storage_power_reward);
        state.this_epoch_reward = TokenAmount::from(5000);
        rt.replace_state(&state);
        rt.set_balance(TokenAmount::from(3500));

        rt.expect_validate_caller_addr(vec![*SYSTEM_ACTOR_ADDR]);
        let expected_reward = TokenAmount::from(1000);
        rt.expect_send(
            *WINNER,
            MinerMethod::ApplyRewards as u64,
            Serialized::serialize(BigIntSer(&expected_reward)).unwrap(),
            expected_reward.clone(),
            Serialized::default(),
            ExitCode::ErrForbidden,
        );
        rt.expect_send(
            *BURNT_FUNDS_ACTOR_ADDR,
            METHOD_SEND,
            Serialized::default(),
            expected_reward,
            Serialized::default(),
            ExitCode::Ok,
        );

        let params = AwardBlockRewardParams {
            miner: *WINNER,
            penalty: TokenAmount::from(0),
            gas_reward: TokenAmount::from(0),
            win_count: 1,
        };

        assert!(rt
            .call(
                &*REWARD_ACTOR_CODE_ID,
                Method::AwardBlockReward as u64,
                &Serialized::serialize(params).unwrap()
            )
            .is_ok());

        rt.verify();
    }
}

mod test_this_epoch_reward {
    use super::*;

    #[test]
    fn successfully_fetch_reward_for_this_epoch() {
        let mut rt = construct_and_verify(&StoragePower::from(1));

        let state: State = rt.get_state().unwrap();

        let resp: ThisEpochRewardReturn = this_epoch_reward(&mut rt);

        assert_eq!(
            state.this_epoch_baseline_power,
            resp.this_epoch_baseline_power
        );
        assert_eq!(
            state.this_epoch_reward_smoothed,
            resp.this_epoch_reward_smoothed
        );
    }
}

#[test]
fn test_successive_kpi_updates() {
    let power = StoragePower::from_i128(1 << 50).unwrap();
    let mut rt = construct_and_verify(&power);

    for i in &[1, 2, 3] {
        rt.epoch = ChainEpoch::from(*i);
        update_network_kpi(&mut rt, &power);
    }
}

fn construct_and_verify(curr_power: &StoragePower) -> MockRuntime {
    let mut rt = MockRuntime {
        receiver: *REWARD_ACTOR_ADDR,
        caller: *SYSTEM_ACTOR_ADDR,
        caller_type: *SYSTEM_ACTOR_CODE_ID,
        ..Default::default()
    };
    rt.expect_validate_caller_addr(vec![*SYSTEM_ACTOR_ADDR]);
    let ret = rt
        .call(
            &*REWARD_ACTOR_CODE_ID,
            METHOD_CONSTRUCTOR,
            &Serialized::serialize(BigIntSer(curr_power)).unwrap(),
        )
        .unwrap();

    assert_eq!(Serialized::default(), ret);
    rt.verify();
    rt
}

fn award_block_reward(
    rt: &mut MockRuntime,
    miner: Address,
    penalty: TokenAmount,
    gas_reward: TokenAmount,
    win_count: i64,
    expected_payment: TokenAmount,
) -> Result<Serialized, ActorError> {
    rt.expect_validate_caller_addr(vec![*SYSTEM_ACTOR_ADDR]);
    let miner_penalty = &penalty * PENALTY_MULTIPLIER;
    rt.expect_send(
        miner,
        MinerMethod::ApplyRewards as u64,
        Serialized::serialize(&ApplyRewardParams {
            reward: expected_payment.clone(),
            penalty: miner_penalty,
        })
        .unwrap(),
        expected_payment.clone(),
        Serialized::default(),
        ExitCode::Ok,
    );

    if penalty > TokenAmount::from(0) {
        rt.expect_send(
            *BURNT_FUNDS_ACTOR_ADDR,
            METHOD_SEND,
            Serialized::default(),
            expected_payment,
            Serialized::default(),
            ExitCode::Ok,
        );
    }

    let params = Serialized::serialize(AwardBlockRewardParams {
        miner,
        penalty,
        gas_reward,
        win_count,
    })
    .unwrap();

    let serialized_bytes = rt.call(
        &*REWARD_ACTOR_CODE_ID,
        Method::AwardBlockReward as u64,
        &params,
    )?;

    rt.verify();
    Ok(serialized_bytes)
}

fn this_epoch_reward(rt: &mut MockRuntime) -> ThisEpochRewardReturn {
    rt.expect_validate_caller_any();
    let serialized_result = rt
        .call(
            &*REWARD_ACTOR_CODE_ID,
            Method::ThisEpochReward as u64,
            &Serialized::default(),
        )
        .unwrap();
    let resp: ThisEpochRewardReturn = Serialized::deserialize(&serialized_result).unwrap();
    rt.verify();
    resp
}

fn update_network_kpi(rt: &mut MockRuntime, curr_raw_power: &StoragePower) {
    rt.set_caller(*POWER_ACTOR_CODE_ID, *STORAGE_POWER_ACTOR_ADDR);
    rt.expect_validate_caller_addr(vec![*STORAGE_POWER_ACTOR_ADDR]);

    let params = &Serialized::serialize(BigIntSer(&curr_raw_power)).unwrap();
    assert!(rt
        .call(
            &*REWARD_ACTOR_CODE_ID,
            Method::UpdateNetworkKPI as u64,
            params
        )
        .is_ok());
    rt.verify();
}
