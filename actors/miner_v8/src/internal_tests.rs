// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use fil_actors_runtime_v8::{test_utils::blake2b_256, EPOCHS_IN_DAY};

#[test]
fn test_assign_proving_period_boundary() {
    let addr1 = Address::new_actor("a".as_bytes());
    let addr2 = Address::new_actor("b".as_bytes());
    let start_epoch = 1;
    let policy = Policy::default();

    // ensure the values are different for different addresses
    let b1 = assign_proving_period_offset(&policy, addr1, start_epoch, blake2b_256).unwrap();
    assert!(b1 >= 0);
    assert!(b1 < policy.wpost_proving_period);

    let b2 = assign_proving_period_offset(&policy, addr2, start_epoch, blake2b_256).unwrap();
    assert!(b2 >= 0);
    assert!(b2 < policy.wpost_proving_period);

    assert_ne!(b1, b2);

    // Ensure boundaries are always less than a proving period.
    for i in 0..10_000 {
        let boundary = assign_proving_period_offset(&policy, addr1, i, blake2b_256).unwrap();
        assert!(boundary >= 0);
        assert!(boundary < policy.wpost_proving_period);
    }
}

#[test]
fn test_current_proving_period_start() {
    let policy = Policy::default();

    // At epoch zero...
    let curr = 0;

    // ... with offset zero, the current proving period starts now, ...
    assert_eq!(0, current_proving_period_start(&policy, curr, 0));

    // ... and all other offsets are negative.
    assert_eq!(-policy.wpost_proving_period + 1, current_proving_period_start(&policy, curr, 1));
    assert_eq!(-policy.wpost_proving_period + 10, current_proving_period_start(&policy, curr, 10));
    assert_eq!(-1, current_proving_period_start(&policy, curr, policy.wpost_proving_period - 1));

    // At epoch 1, offsets 0 and 1 start at offset, but offsets 2 and later start in the past.
    let curr = 1;
    assert_eq!(0, current_proving_period_start(&policy, curr, 0));
    assert_eq!(1, current_proving_period_start(&policy, curr, 1));
    assert_eq!(-policy.wpost_proving_period + 2, current_proving_period_start(&policy, curr, 2));
    assert_eq!(-policy.wpost_proving_period + 3, current_proving_period_start(&policy, curr, 3));
    assert_eq!(-1, current_proving_period_start(&policy, curr, policy.wpost_proving_period - 1));

    // An arbitrary mid-period epoch.
    let curr = 123;
    assert_eq!(0, current_proving_period_start(&policy, curr, 0));
    assert_eq!(1, current_proving_period_start(&policy, curr, 1));
    assert_eq!(122, current_proving_period_start(&policy, curr, 122));
    assert_eq!(123, current_proving_period_start(&policy, curr, 123));
    assert_eq!(
        -policy.wpost_proving_period + 124,
        current_proving_period_start(&policy, curr, 124)
    );
    assert_eq!(-1, current_proving_period_start(&policy, curr, policy.wpost_proving_period - 1));

    // The final epoch in the chain's first full period
    let curr = policy.wpost_proving_period - 1;
    assert_eq!(0, current_proving_period_start(&policy, curr, 0));
    assert_eq!(1, current_proving_period_start(&policy, curr, 1));
    assert_eq!(2, current_proving_period_start(&policy, curr, 2));
    assert_eq!(
        policy.wpost_proving_period - 2,
        current_proving_period_start(&policy, curr, policy.wpost_proving_period - 2),
    );
    assert_eq!(
        policy.wpost_proving_period - 1,
        current_proving_period_start(&policy, curr, policy.wpost_proving_period - 1),
    );

    // Into the chain's second period
    let curr = policy.wpost_proving_period;
    assert_eq!(policy.wpost_proving_period, current_proving_period_start(&policy, curr, 0));
    assert_eq!(1, current_proving_period_start(&policy, curr, 1));
    assert_eq!(2, current_proving_period_start(&policy, curr, 2));
    assert_eq!(
        policy.wpost_proving_period - 1,
        current_proving_period_start(&policy, curr, policy.wpost_proving_period - 1)
    );

    let curr = policy.wpost_proving_period + 234;
    assert_eq!(policy.wpost_proving_period, current_proving_period_start(&policy, curr, 0));
    assert_eq!(policy.wpost_proving_period + 1, current_proving_period_start(&policy, curr, 1));
    assert_eq!(policy.wpost_proving_period + 233, current_proving_period_start(&policy, curr, 233));
    assert_eq!(policy.wpost_proving_period + 234, current_proving_period_start(&policy, curr, 234));
    assert_eq!(235, current_proving_period_start(&policy, curr, 235));
    assert_eq!(
        policy.wpost_proving_period - 1,
        current_proving_period_start(&policy, curr, policy.wpost_proving_period - 1)
    );
}

// constant filter estimate cumsum ratio is just multiplication and division
// test that internal precision of BR calculation does not cost accuracy
// compared to simple multiplication in this case.
#[test]
fn br_looks_right_in_plausible_sector_power_network_power_reward_range() {
    // between 10 and 100 FIL is reasonable for near-mid future
    let tens_of_fil: TokenAmount = TokenAmount::from_whole(50);
    let reward_estimate = FilterEstimate::new(tens_of_fil.atto().clone(), Zero::zero());
    let small_power = StoragePower::from(32_u64 << 30); // 32 GiB
    let huge_power = StoragePower::from(1_u64 << 60); // 1 EiB
    let small_power_br_num = &small_power * EPOCHS_IN_DAY * &tens_of_fil;
    let huge_power_br_num = &huge_power * EPOCHS_IN_DAY * &tens_of_fil;

    // QAPower = Space * AverageQuality
    // 10s of EiBs -- lower range
    // 1.2e18 * 10 bytes * 1 quality ~ 1e19
    let tens_of_eibs: StoragePower = StoragePower::from(10).pow(18) * 10;
    let low_power_estimate = FilterEstimate::new(tens_of_eibs.clone(), Zero::zero());
    let br_small_low = expected_reward_for_power(
        &reward_estimate,
        &low_power_estimate,
        &small_power,
        EPOCHS_IN_DAY,
    );
    let br_huge_low = expected_reward_for_power(
        &reward_estimate,
        &low_power_estimate,
        &huge_power,
        EPOCHS_IN_DAY,
    );
    assert_eq!(small_power_br_num.div_floor(tens_of_eibs.clone()), br_small_low);
    assert_eq!(huge_power_br_num.div_floor(tens_of_eibs), br_huge_low);

    // 100s of EiBs
    // 1.2e18 * 100 bytes * 5 quality ~ 6e20
    let hundreds_of_eibs: StoragePower = StoragePower::from(10).pow(18) * 600;
    let mid_power_estimate = FilterEstimate::new(hundreds_of_eibs.clone(), Zero::zero());
    let br_small_mid = expected_reward_for_power(
        &reward_estimate,
        &mid_power_estimate,
        &small_power,
        EPOCHS_IN_DAY,
    );
    let br_huge_mid = expected_reward_for_power(
        &reward_estimate,
        &mid_power_estimate,
        &huge_power,
        EPOCHS_IN_DAY,
    );
    assert_eq!(small_power_br_num.div_floor(hundreds_of_eibs.clone()), br_small_mid);
    assert_eq!(huge_power_br_num.div_floor(hundreds_of_eibs), br_huge_mid);

    // 1000s of EiBs -- upper range
    // 1.2e18 * 1000 bytes * 10 quality = 1.2e22 ~ 2e22
    let thousands_of_eibs: StoragePower = StoragePower::from(10).pow(18) * 20000;
    let upper_power_estimate = FilterEstimate::new(thousands_of_eibs.clone(), Zero::zero());
    let br_small_upper = expected_reward_for_power(
        &reward_estimate,
        &upper_power_estimate,
        &small_power,
        EPOCHS_IN_DAY,
    );
    let br_huge_upper = expected_reward_for_power(
        &reward_estimate,
        &upper_power_estimate,
        &huge_power,
        EPOCHS_IN_DAY,
    );
    assert_eq!(small_power_br_num.div_floor(thousands_of_eibs.clone()), br_small_upper);
    assert_eq!(huge_power_br_num.div_floor(thousands_of_eibs), br_huge_upper);
}

#[test]
fn declared_and_undeclared_fault_penalties_are_linear_over_sector_qa_power_term() {
    // Construct plausible reward and qa power filtered estimates
    let epoch_reward = TokenAmount::from_atto(100_u64 << 53);
    // not too much growth over ~3000 epoch projection in BR
    let reward_estimate = FilterEstimate::new(epoch_reward.atto().clone(), Zero::zero());

    let network_power = StoragePower::from(100_u64 << 50);
    let power_estimate = FilterEstimate::new(network_power, Zero::zero());

    let faulty_sector_a_power = StoragePower::from(1_u64 << 50);
    let faulty_sector_b_power = StoragePower::from(19_u64 << 50);
    let faulty_sector_c_power = StoragePower::from(63_u64 << 50);
    let total_fault_power: StoragePower =
        &faulty_sector_a_power + &faulty_sector_b_power + &faulty_sector_c_power;

    // Declared faults
    let ff_a = pledge_penalty_for_continued_fault(
        &reward_estimate,
        &power_estimate,
        &faulty_sector_a_power,
    );
    let ff_b = pledge_penalty_for_continued_fault(
        &reward_estimate,
        &power_estimate,
        &faulty_sector_b_power,
    );
    let ff_c = pledge_penalty_for_continued_fault(
        &reward_estimate,
        &power_estimate,
        &faulty_sector_c_power,
    );

    let ff_all =
        pledge_penalty_for_continued_fault(&reward_estimate, &power_estimate, &total_fault_power);

    // Because we can introduce rounding error between 1 and zero for every penalty calculation
    // we can at best expect n calculations of 1 power to be within n of 1 calculation of n powers.
    let diff = &ff_all - (&ff_c + &ff_a + &ff_b);
    assert!(diff >= Zero::zero());
    assert!(diff < TokenAmount::from_atto(3));

    // Undeclared faults
    let sp_a = pledge_penalty_for_termination_lower_bound(
        &reward_estimate,
        &power_estimate,
        &faulty_sector_a_power,
    );
    let sp_b = pledge_penalty_for_termination_lower_bound(
        &reward_estimate,
        &power_estimate,
        &faulty_sector_b_power,
    );
    let sp_c = pledge_penalty_for_termination_lower_bound(
        &reward_estimate,
        &power_estimate,
        &faulty_sector_c_power,
    );

    let sp_all = pledge_penalty_for_termination_lower_bound(
        &reward_estimate,
        &power_estimate,
        &total_fault_power,
    );

    // Because we can introduce rounding error between 1 and zero for every penalty calculation
    // we can at best expect n calculations of 1 power to be within n of 1 calculation of n powers.
    let diff = &sp_all - (&sp_c + &sp_a + &sp_b);
    assert!(diff >= Zero::zero());
    assert!(diff < TokenAmount::from_atto(3));
}
