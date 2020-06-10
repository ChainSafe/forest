// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(unused_imports)]

use actor::miner::{
    compute_proving_period_deadline, FAULT_DECLARATION_CUTOFF, WPOST_CHALLENGE_WINDOW,
    WPOST_PERIOD_DEADLINES, WPOST_PROVING_PERIOD,
};
use clock::ChainEpoch;

#[test]
fn test_pre_open_deadlines() {
    // Current is before the period opens.
    let curr: ChainEpoch = 20;
    let mut period_start = FAULT_DECLARATION_CUTOFF + 1;
    let mut di = compute_proving_period_deadline(period_start, curr);
    assert_eq!(0, di.index);
    assert_eq!(period_start, di.open);

    assert!(!di.period_started());
    assert!(!di.is_open());
    assert!(!di.has_elapsed());
    assert!(!di.fault_cutoff_passed());
    assert_eq!(period_start + WPOST_PROVING_PERIOD - 1, di.period_end());
    assert_eq!(period_start + WPOST_PROVING_PERIOD, di.next_period_start());

    period_start = FAULT_DECLARATION_CUTOFF - 1;
    di = compute_proving_period_deadline(period_start, curr);
    assert!(di.fault_cutoff_passed());
}
