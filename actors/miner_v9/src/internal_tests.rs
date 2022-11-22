// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use fil_actors_runtime_v9::EPOCHS_IN_DAY;
use fvm_shared::econ::TokenAmount;
use fvm_shared::smooth::FilterEstimate;
use num_traits::Zero;

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
    assert_eq!(
        small_power_br_num.div_floor(tens_of_eibs.clone()),
        br_small_low
    );
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
    assert_eq!(
        small_power_br_num.div_floor(hundreds_of_eibs.clone()),
        br_small_mid
    );
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
    assert_eq!(
        small_power_br_num.div_floor(thousands_of_eibs.clone()),
        br_small_upper
    );
    assert_eq!(
        huge_power_br_num.div_floor(thousands_of_eibs),
        br_huge_upper
    );
}

#[test]
fn declared_and_undeclared_fault_penalties_are_linear_over_sector_qa_power_term() {
    // Construct plausible reward and qa power filtered estimates
    let epoch_reward = BigInt::from(100_u64 << 53);
    // not too much growth over ~3000 epoch projection in BR
    let reward_estimate = FilterEstimate::new(epoch_reward, Zero::zero());

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
