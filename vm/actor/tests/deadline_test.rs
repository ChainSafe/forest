// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(unused_imports)]
use actor::miner::{
    compute_proving_period_deadline, FAULT_DECLARATION_CUTOFF, WPOST_CHALLENGE_WINDOW,
    WPOST_PERIOD_DEADLINES, WPOST_PROVING_PERIOD, WPOST_CHALLENGE_LOOKBACK, DeadlineInfo, Deadlines, partitions_for_deadline
};
use clock::ChainEpoch;
use bitfield::BitField;

fn assert_deadline_info(current: ChainEpoch, period_start: ChainEpoch, index: usize, expected_deadline_open: ChainEpoch) -> DeadlineInfo {
    let di = DeadlineInfo {
        current_epoch: current, 
        period_start,
        index,
        open: expected_deadline_open,
        close: expected_deadline_open + WPOST_CHALLENGE_WINDOW,
        challenge: expected_deadline_open - WPOST_CHALLENGE_LOOKBACK,
        fault_cutoff: expected_deadline_open - FAULT_DECLARATION_CUTOFF,
    };
    let actual = compute_proving_period_deadline(period_start, current);
    assert!(actual.period_started());
    assert!(actual.is_open());
    assert!(!actual.has_elapsed());
    assert_eq!(di, actual);
    return di
}

#[test]
fn pre_open_deadlines_test() {
    // Current is before the period opens.
    let curr: ChainEpoch = 0;
    let mut period_start = FAULT_DECLARATION_CUTOFF + 1;
    let mut di = compute_proving_period_deadline(period_start, curr);
    assert_eq!(0, di.index);
    assert_eq!(period_start, di.open);
    assert!(!di.period_started());
    assert!(!di.is_open());
    assert!(!di.has_elapsed());
    assert!(!di.fault_cutoff_passed(), format!("curr epoch: {} >= faultcutoff: {}", di.current_epoch, di.fault_cutoff));
    assert_eq!(period_start + WPOST_PROVING_PERIOD - 1, di.period_end());
    assert_eq!(period_start + WPOST_PROVING_PERIOD, di.next_period_start());
    period_start = FAULT_DECLARATION_CUTOFF - 1;
    di = compute_proving_period_deadline(period_start, curr);
    assert!(di.fault_cutoff_passed());
}

#[test]
fn offset_zero_test() {
    let first_period_start: ChainEpoch = 0;
    
    let mut di = assert_deadline_info(0, first_period_start, 0, 0);
    assert_eq!(WPOST_CHALLENGE_LOOKBACK - WPOST_CHALLENGE_LOOKBACK, di.challenge);
    assert_eq!(FAULT_DECLARATION_CUTOFF - FAULT_DECLARATION_CUTOFF, di.fault_cutoff);
    assert!(di.is_open());
    assert!(di.fault_cutoff_passed());

    assert_deadline_info(1, first_period_start, 0, 0);
    assert_deadline_info(WPOST_CHALLENGE_WINDOW - 1, first_period_start, 0, 0);
    assert_deadline_info(WPOST_CHALLENGE_WINDOW, first_period_start, 1, WPOST_CHALLENGE_WINDOW);
    assert_deadline_info(WPOST_CHALLENGE_WINDOW, first_period_start, 1, WPOST_CHALLENGE_WINDOW);
    assert_deadline_info(WPOST_CHALLENGE_WINDOW * 2 - 1, first_period_start, 1, WPOST_CHALLENGE_WINDOW);
    assert_deadline_info(WPOST_CHALLENGE_WINDOW * 2, first_period_start, 2, WPOST_CHALLENGE_WINDOW * 2);
    assert_deadline_info(WPOST_PROVING_PERIOD - 1, first_period_start, WPOST_PERIOD_DEADLINES - 1, WPOST_PROVING_PERIOD - WPOST_CHALLENGE_WINDOW);

    // Second proving period
    // First epoch of deadline 0
    let second_period_start = WPOST_PROVING_PERIOD;
    di = assert_deadline_info(WPOST_PROVING_PERIOD, second_period_start, 0, WPOST_PROVING_PERIOD);
    assert_eq!(WPOST_PROVING_PERIOD - WPOST_CHALLENGE_LOOKBACK, di.challenge);
    assert_eq!(WPOST_PROVING_PERIOD - FAULT_DECLARATION_CUTOFF, di.fault_cutoff);

    // final epoch of deadline 0.
    assert_deadline_info(WPOST_PROVING_PERIOD+WPOST_CHALLENGE_WINDOW - 1, second_period_start, 0, WPOST_PROVING_PERIOD+0);
    // first epoch of deadline 1
    assert_deadline_info(WPOST_PROVING_PERIOD+WPOST_CHALLENGE_WINDOW, second_period_start, 1, WPOST_PROVING_PERIOD+WPOST_CHALLENGE_WINDOW);
    assert_deadline_info(WPOST_PROVING_PERIOD+WPOST_CHALLENGE_WINDOW + 1, second_period_start, 1, WPOST_PROVING_PERIOD+WPOST_CHALLENGE_WINDOW);
}

#[test]
fn offset_non_zero_test() {
    // Arbitrary not aligned with challenge window.
    let offset = WPOST_CHALLENGE_WINDOW * 2 + 2;
    let initial_pp_start = offset - WPOST_PROVING_PERIOD;
    let val = (offset / WPOST_CHALLENGE_WINDOW) as usize;
    let first_di_index = WPOST_PERIOD_DEADLINES - val - 1;
    let first_di_open = initial_pp_start + WPOST_CHALLENGE_WINDOW * first_di_index as i64;
    
    assert!(offset < WPOST_PROVING_PERIOD);
    assert!(initial_pp_start < 0);
    assert!(first_di_open < 0);

    // Incomplete initial proving period.
	// At epoch zero, the initial deadlines in the period have already passed and we're part way through
    // another one.
    let di = assert_deadline_info(0, initial_pp_start, first_di_index, first_di_open);
    assert_eq!(first_di_open - WPOST_CHALLENGE_LOOKBACK, di.challenge);
    assert_eq!(first_di_open - FAULT_DECLARATION_CUTOFF, di.fault_cutoff);
    assert!(di.is_open());
    assert!(di.fault_cutoff_passed());

    // Epoch 1 
    assert_deadline_info(1, initial_pp_start, first_di_index, first_di_open);

    // epoch 2 rolled over to third last challenge window
    assert_deadline_info(2, initial_pp_start, first_di_index + 1, first_di_open + WPOST_CHALLENGE_WINDOW);
    assert_deadline_info(3, initial_pp_start, first_di_index + 1, first_di_open + WPOST_CHALLENGE_WINDOW);

    // last epoch of second last window
    assert_deadline_info(2+WPOST_CHALLENGE_WINDOW-1, initial_pp_start, first_di_index + 1, first_di_open + WPOST_CHALLENGE_WINDOW);
    // first epoch of last challenge window
    assert_deadline_info(2+WPOST_CHALLENGE_WINDOW, initial_pp_start, first_di_index + 2, first_di_open + WPOST_CHALLENGE_WINDOW * 2);
    // last epoch of last challenge window
    assert_eq!(WPOST_PERIOD_DEADLINES - 1, first_di_index + 2);
    assert_deadline_info(2+2*WPOST_CHALLENGE_WINDOW-1, initial_pp_start, first_di_index + 2, first_di_open + WPOST_CHALLENGE_WINDOW * 2);

    // first epoch of next proving period
    assert_deadline_info(2+2*WPOST_CHALLENGE_WINDOW, initial_pp_start + WPOST_PROVING_PERIOD, 0, initial_pp_start+ WPOST_PROVING_PERIOD);
    assert_deadline_info(2+2*WPOST_CHALLENGE_WINDOW+1, initial_pp_start + WPOST_PROVING_PERIOD, 0, initial_pp_start+ WPOST_PROVING_PERIOD);
}

#[test]
fn period_expired() {
    let offset: ChainEpoch = 1;
    let d = compute_proving_period_deadline(offset, offset + WPOST_PROVING_PERIOD);
    assert!(d.period_started());
    assert!(d.period_elapsed());
    assert_eq!(WPOST_PERIOD_DEADLINES, d.index);
    assert!(!d.is_open());
    assert!(d.has_elapsed());
    assert!(d.fault_cutoff_passed());
    assert_eq!(offset+WPOST_PROVING_PERIOD-1, d.period_end());
    assert_eq!(offset+WPOST_PROVING_PERIOD, d.next_period_start());
}

const PART_SIZE: usize = 1000;

#[test]
fn empty_deadlines_test() {
    
    let empty: &[u64] = &[];
    let mut dl = build_deadlines(empty);
    let (first_idx, sector_count) = partitions_for_deadline(&mut dl, PART_SIZE, 0).unwrap();
    assert_eq!(0, first_idx);
    assert_eq!(0, sector_count);

    let (sec_index, sec_count) = partitions_for_deadline(&mut dl, PART_SIZE, WPOST_PERIOD_DEADLINES - 1).unwrap();
    assert_eq!(0, sec_index);
    assert_eq!(0, sec_count);
}

#[test]
fn single_sector_test() {
    let single: &[u64] = &[1];
    let mut dl = build_deadlines(single);

    let (first_idx, sector_count) = partitions_for_deadline(&mut dl, PART_SIZE, 0).unwrap();
    assert_eq!(0, first_idx);
    assert_eq!(1, sector_count);

    let (sec_idx, sec_count) = partitions_for_deadline(&mut dl, PART_SIZE, 1).unwrap();
    assert_eq!(1, sec_idx);
    assert_eq!(0, sec_count);
}
fn build_deadlines(gen: &[u64]) -> Deadlines {
    DeadlineBuilder::new(gen).deadlines
}

fn seq(first: usize, _count: usize) -> Vec<u64> {
    let mut values: Vec<u64> = Vec::new();
    for (i, val) in values.iter_mut().enumerate() {
        *val = first as u64 + i as u64;
    }
    return values
}

fn fb_seq(first: usize, count: usize) -> BitField {
    let values = seq(first, count);
    BitField::new_from_set(&values)
}

/// A builder for initialising a Deadlines with sectors assigned.
struct DeadlineBuilder {
    deadlines: Deadlines,
    next_sector_idx: usize
}

impl DeadlineBuilder {
    fn new(counts: &[u64]) -> Self {
        let mut di = DeadlineBuilder {
            deadlines: Deadlines::new(),
            next_sector_idx: 0
        };
        di.add_to_form(0, counts);
        di
    }
    fn add_to(&mut self, idx: usize, count: usize) {
        let nums = seq(self.next_sector_idx, count);
        self.next_sector_idx += count;
        self.deadlines.add_to_deadline(idx, &nums).unwrap();
    }

    fn add_to_form(&mut self, first: usize, counts: &[u64]) {
        for (i, c) in counts.into_iter().enumerate() {
            self.add_to(first+i, *c as usize);
        }
    }

    fn add_to_all(&mut self, count: usize) {
        todo!();
    }

    fn add_to_all_form(&mut self, first: usize, count:usize) {
        let mut i = first;
        while i < WPOST_PERIOD_DEADLINES {
            self.add_to(i, count);
            i += 1;
        }
    }
}
