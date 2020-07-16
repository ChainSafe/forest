// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::miner::{
    assign_new_sectors, compute_partitions_sector, compute_proving_period_deadline,
    partitions_for_deadline, DeadlineInfo, Deadlines, FAULT_DECLARATION_CUTOFF,
    WPOST_CHALLENGE_LOOKBACK, WPOST_CHALLENGE_WINDOW, WPOST_PERIOD_DEADLINES, WPOST_PROVING_PERIOD,
};
use bitfield::BitField;
use clock::ChainEpoch;

fn assert_deadline_info(
    current: ChainEpoch,
    period_start: ChainEpoch,
    index: usize,
    expected_deadline_open: ChainEpoch,
) -> DeadlineInfo {
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
    return di;
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
    assert!(
        !di.fault_cutoff_passed(),
        format!(
            "curr epoch: {} >= faultcutoff: {}",
            di.current_epoch, di.fault_cutoff
        )
    );
    assert_eq!(period_start + WPOST_PROVING_PERIOD - 1, di.period_end());
    assert_eq!(period_start + WPOST_PROVING_PERIOD, di.next_period_start());
    period_start = FAULT_DECLARATION_CUTOFF - 1;
    di = compute_proving_period_deadline(period_start, curr);
    assert!(di.fault_cutoff_passed());
}

#[test]
fn offset_zero_test() {
    let first_period_start: ChainEpoch = 0;
    // First proving period.
    let mut di = assert_deadline_info(0, first_period_start, 0, 0);
    assert_eq!(-WPOST_CHALLENGE_LOOKBACK, di.challenge);
    assert_eq!(-FAULT_DECLARATION_CUTOFF, di.fault_cutoff);
    assert!(di.is_open());
    assert!(di.fault_cutoff_passed());

    assert_deadline_info(1, first_period_start, 0, 0);
    // Final epoch of deadline 0.
    assert_deadline_info(WPOST_CHALLENGE_WINDOW - 1, first_period_start, 0, 0);
    // First epoch of deadline 1
    assert_deadline_info(
        WPOST_CHALLENGE_WINDOW,
        first_period_start,
        1,
        WPOST_CHALLENGE_WINDOW,
    );
    assert_deadline_info(
        WPOST_CHALLENGE_WINDOW + 1,
        first_period_start,
        1,
        WPOST_CHALLENGE_WINDOW,
    );
    // Final epoch of deadline 1
    assert_deadline_info(
        WPOST_CHALLENGE_WINDOW * 2 - 1,
        first_period_start,
        1,
        WPOST_CHALLENGE_WINDOW,
    );
    // First epoch of deadline 2
    assert_deadline_info(
        WPOST_CHALLENGE_WINDOW * 2,
        first_period_start,
        2,
        WPOST_CHALLENGE_WINDOW * 2,
    );
    // Last epoch of last deadline
    assert_deadline_info(
        WPOST_PROVING_PERIOD - 1,
        first_period_start,
        WPOST_PERIOD_DEADLINES - 1,
        WPOST_PROVING_PERIOD - WPOST_CHALLENGE_WINDOW,
    );

    // Second proving period
    // First epoch of deadline 0
    let second_period_start = WPOST_PROVING_PERIOD;
    di = assert_deadline_info(
        WPOST_PROVING_PERIOD,
        second_period_start,
        0,
        WPOST_PROVING_PERIOD,
    );
    assert_eq!(
        WPOST_PROVING_PERIOD - WPOST_CHALLENGE_LOOKBACK,
        di.challenge
    );
    assert_eq!(
        WPOST_PROVING_PERIOD - FAULT_DECLARATION_CUTOFF,
        di.fault_cutoff
    );

    // final epoch of deadline 0.
    assert_deadline_info(
        WPOST_PROVING_PERIOD + WPOST_CHALLENGE_WINDOW - 1,
        second_period_start,
        0,
        WPOST_PROVING_PERIOD + 0,
    );
    // first epoch of deadline 1
    assert_deadline_info(
        WPOST_PROVING_PERIOD + WPOST_CHALLENGE_WINDOW,
        second_period_start,
        1,
        WPOST_PROVING_PERIOD + WPOST_CHALLENGE_WINDOW,
    );
    assert_deadline_info(
        WPOST_PROVING_PERIOD + WPOST_CHALLENGE_WINDOW + 1,
        second_period_start,
        1,
        WPOST_PROVING_PERIOD + WPOST_CHALLENGE_WINDOW,
    );
}

#[test]
fn offset_non_zero_test() {
    // Arbitrary not aligned with challenge window.
    let offset = WPOST_CHALLENGE_WINDOW * 2 + 2;
    let initial_pp_start = offset - WPOST_PROVING_PERIOD;
    let val = (offset / WPOST_CHALLENGE_WINDOW) as usize;
    let first_dl_index = WPOST_PERIOD_DEADLINES - val - 1;
    let first_dl_open = initial_pp_start + WPOST_CHALLENGE_WINDOW * first_dl_index as i64;

    assert!(offset < WPOST_PROVING_PERIOD);
    assert!(initial_pp_start < 0);
    assert!(first_dl_open < 0);

    // Incomplete initial proving period.
    // At epoch zero, the initial deadlines in the period have already passed and we're part way through
    // another one.
    let di = assert_deadline_info(0, initial_pp_start, first_dl_index, first_dl_open);
    assert_eq!(first_dl_open - WPOST_CHALLENGE_LOOKBACK, di.challenge);
    assert_eq!(first_dl_open - FAULT_DECLARATION_CUTOFF, di.fault_cutoff);
    assert!(di.is_open());
    assert!(di.fault_cutoff_passed());

    // Epoch 1
    assert_deadline_info(1, initial_pp_start, first_dl_index, first_dl_open);

    // epoch 2 rolled over to third last challenge window
    assert_deadline_info(
        2,
        initial_pp_start,
        first_dl_index + 1,
        first_dl_open + WPOST_CHALLENGE_WINDOW,
    );
    assert_deadline_info(
        3,
        initial_pp_start,
        first_dl_index + 1,
        first_dl_open + WPOST_CHALLENGE_WINDOW,
    );

    // last epoch of second last window
    assert_deadline_info(
        2 + WPOST_CHALLENGE_WINDOW - 1,
        initial_pp_start,
        first_dl_index + 1,
        first_dl_open + WPOST_CHALLENGE_WINDOW,
    );
    // first epoch of last challenge window
    assert_deadline_info(
        2 + WPOST_CHALLENGE_WINDOW,
        initial_pp_start,
        first_dl_index + 2,
        first_dl_open + WPOST_CHALLENGE_WINDOW * 2,
    );
    // last epoch of last challenge window
    assert_eq!(WPOST_PERIOD_DEADLINES - 1, first_dl_index + 2);
    assert_deadline_info(
        2 + 2 * WPOST_CHALLENGE_WINDOW - 1,
        initial_pp_start,
        first_dl_index + 2,
        first_dl_open + WPOST_CHALLENGE_WINDOW * 2,
    );

    // first epoch of next proving period
    assert_deadline_info(
        2 + 2 * WPOST_CHALLENGE_WINDOW,
        initial_pp_start + WPOST_PROVING_PERIOD,
        0,
        initial_pp_start + WPOST_PROVING_PERIOD,
    );
    assert_deadline_info(
        2 + 2 * WPOST_CHALLENGE_WINDOW + 1,
        initial_pp_start + WPOST_PROVING_PERIOD,
        0,
        initial_pp_start + WPOST_PROVING_PERIOD,
    );
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
    assert_eq!(offset + WPOST_PROVING_PERIOD - 1, d.period_end());
    assert_eq!(offset + WPOST_PROVING_PERIOD, d.next_period_start());
}

const PART_SIZE: usize = 1000;

#[test]
fn empty_deadlines_test() {
    let empty: &[usize] = &[];
    let mut dl = build_deadlines(empty);
    let (first_idx, sector_count) = partitions_for_deadline(&mut dl, PART_SIZE, 0).unwrap();
    assert_eq!(0, first_idx);
    assert_eq!(0, sector_count);

    let (sec_index, sec_count) =
        partitions_for_deadline(&mut dl, PART_SIZE, WPOST_PERIOD_DEADLINES - 1).unwrap();
    assert_eq!(0, sec_index);
    assert_eq!(0, sec_count);
}

#[test]
fn single_sector_test() {
    let single: &[usize] = &[1];
    let mut dl = build_deadlines(single);
    let (first_idx, sector_count) = partitions_for_deadline(&mut dl, PART_SIZE, 0).unwrap();
    assert_eq!(0, first_idx);
    assert_eq!(1, sector_count);

    let (second_idx, second_count) = partitions_for_deadline(&mut dl, PART_SIZE, 1).unwrap();
    assert_eq!(1, second_idx);
    assert_eq!(0, second_count);

    let (third_idx, third_count) =
        partitions_for_deadline(&mut dl, PART_SIZE, WPOST_PERIOD_DEADLINES - 1).unwrap();
    assert_eq!(1, third_idx);
    assert_eq!(0, third_count);
}

#[test]
fn single_sector_not_zero_deadline() {
    let sector: &[usize] = &[0, 1];
    let mut dl = build_deadlines(sector);

    let (first_idx, sector_count) = partitions_for_deadline(&mut dl, PART_SIZE, 0).unwrap();
    assert_eq!(0, first_idx);
    assert_eq!(0, sector_count);

    let (second_idx, second_count) = partitions_for_deadline(&mut dl, PART_SIZE, 1).unwrap();
    assert_eq!(0, second_idx);
    assert_eq!(1, second_count);

    let (third_idx, third_count) = partitions_for_deadline(&mut dl, PART_SIZE, 2).unwrap();
    assert_eq!(1, third_idx);
    assert_eq!(0, third_count);

    let (fourth_idx, fourth_count) =
        partitions_for_deadline(&mut dl, PART_SIZE, WPOST_PERIOD_DEADLINES - 1).unwrap();
    assert_eq!(1, fourth_idx);
    assert_eq!(0, fourth_count);
}

#[test]
fn deadlines_full_partition_test() {
    let mut dl = DeadlineBuilder::new(&[]).add_to_all(PART_SIZE).deadlines;
    let (first_idx, sector_count) = partitions_for_deadline(&mut dl, PART_SIZE, 0).unwrap();
    assert_eq!(0, first_idx);
    assert_eq!(PART_SIZE, sector_count as usize);

    let (second_idx, second_count) = partitions_for_deadline(&mut dl, PART_SIZE, 1).unwrap();
    assert_eq!(1, second_idx);
    assert_eq!(PART_SIZE, second_count as usize);

    let (third_idx, third_count) =
        partitions_for_deadline(&mut dl, PART_SIZE, WPOST_PERIOD_DEADLINES - 1).unwrap();
    assert_eq!(WPOST_PERIOD_DEADLINES - 1, third_idx as usize);
    assert_eq!(PART_SIZE, third_count as usize);
}

#[test]
fn partial_partitions_test() {
    let mut dl = build_deadlines(&[
        PART_SIZE - 1,
        PART_SIZE,
        PART_SIZE - 2,
        PART_SIZE,
        PART_SIZE - 3,
        PART_SIZE,
    ]);
    let (first_idx, sector_count) = partitions_for_deadline(&mut dl, PART_SIZE, 0).unwrap();
    assert_eq!(0, first_idx);
    assert_eq!(PART_SIZE - 1, sector_count as usize);

    let (second_idx, second_count) = partitions_for_deadline(&mut dl, PART_SIZE, 1).unwrap();
    assert_eq!(1, second_idx);
    assert_eq!(PART_SIZE, second_count as usize);

    let (third_idx, third_count) = partitions_for_deadline(&mut dl, PART_SIZE, 2).unwrap();
    assert_eq!(2, third_idx as usize);
    assert_eq!(PART_SIZE - 2, third_count as usize);

    let (fourth_idx, fourth_count) = partitions_for_deadline(&mut dl, PART_SIZE, 5).unwrap();
    assert_eq!(5, fourth_idx as usize);
    assert_eq!(PART_SIZE, fourth_count as usize);
}

#[test]
fn multiple_partitions_test() {
    let mut dl = build_deadlines(&[
        PART_SIZE,
        (PART_SIZE * 2),
        (PART_SIZE * 4 - 1),
        (PART_SIZE * 6),
        (PART_SIZE * 8 - 1),
        (PART_SIZE * 9),
    ]);

    let (first_idx, sector_count) = partitions_for_deadline(&mut dl, PART_SIZE, 0).unwrap();
    assert_eq!(0, first_idx);
    assert_eq!(PART_SIZE, sector_count as usize);

    let (second_idx, second_count) = partitions_for_deadline(&mut dl, PART_SIZE, 1).unwrap();
    assert_eq!(1, second_idx);
    assert_eq!(PART_SIZE * 2, second_count as usize);

    let (third_idx, third_count) = partitions_for_deadline(&mut dl, PART_SIZE, 2).unwrap();
    assert_eq!(3, third_idx);
    assert_eq!(PART_SIZE * 4 - 1, third_count as usize);

    let (fourth_idx, fourth_count) = partitions_for_deadline(&mut dl, PART_SIZE, 3).unwrap();
    assert_eq!(7, fourth_idx);
    assert_eq!(PART_SIZE * 6, fourth_count as usize);

    let (fifth_idx, fifth_count) = partitions_for_deadline(&mut dl, PART_SIZE, 4).unwrap();
    assert_eq!(13, fifth_idx);
    assert_eq!(PART_SIZE * 8 - 1, fifth_count as usize);

    let (sixth_idx, sixth_count) = partitions_for_deadline(&mut dl, PART_SIZE, 5).unwrap();
    assert_eq!(21, sixth_idx);
    assert_eq!(PART_SIZE * 9, sixth_count as usize);

    let (third_idx, third_count) =
        partitions_for_deadline(&mut dl, PART_SIZE, WPOST_PERIOD_DEADLINES - 1).unwrap();
    assert_eq!(30, third_idx as usize);
    assert_eq!(0, third_count as usize);
}
#[test]
#[should_panic(expected = r#"invalid partition 0 at deadline 0 with first 0, count 0"#)]
fn zero_partitions_at_empty_deadline_test() {
    let mut dls = Deadlines::new();
    dls.due[1] = bf_seq(0, 1);

    // No partitions at deadline 0
    compute_partitions_sector(&mut dls, PART_SIZE as u64, 0, &[0]).unwrap();
    compute_partitions_sector(&mut dls, PART_SIZE as u64, 2, &[0]).unwrap();
    compute_partitions_sector(&mut dls, PART_SIZE as u64, 2, &[1]).unwrap();
    compute_partitions_sector(&mut dls, PART_SIZE as u64, 2, &[2]).unwrap();
}

#[test]
fn single_sector_partition_test() {
    let mut dls = Deadlines::new();
    dls.due[1] = bf_seq(0, 1);
    let partitions = compute_partitions_sector(&mut dls, PART_SIZE as u64, 1, &[0]).unwrap();
    assert_eq!(1, partitions.len());

    assert_bf_equal(&bf_seq(0, 1), partitions.get(0).unwrap());
}

#[test]
fn full_partition_test() {
    let mut dls = Deadlines::new();
    dls.due[10] = bf_seq(1234, PART_SIZE);

    let partitions = compute_partitions_sector(&mut dls, PART_SIZE as u64, 10, &[0]).unwrap();
    assert_eq!(1, partitions.len());

    assert_bf_equal(&bf_seq(1234, PART_SIZE), partitions.get(0).unwrap());
}

#[test]
fn full_plus_partial_test() {
    let mut dls = Deadlines::new();
    dls.due[10] = bf_seq(5555, PART_SIZE + 1);

    let mut partitions = compute_partitions_sector(&mut dls, PART_SIZE as u64, 10, &[0]).unwrap();
    assert_eq!(1, partitions.len());
    assert_bf_equal(&bf_seq(5555, PART_SIZE), partitions.get(0).unwrap());

    partitions = compute_partitions_sector(&mut dls, PART_SIZE as u64, 10, &[1]).unwrap();
    assert_eq!(1, partitions.len());
    assert_bf_equal(&bf_seq(5555 + PART_SIZE, 1), partitions.get(0).unwrap());

    partitions = compute_partitions_sector(&mut dls, PART_SIZE as u64, 10, &[0, 1]).unwrap();
    assert_eq!(2, partitions.len());
    assert_bf_equal(&bf_seq(5555, PART_SIZE), partitions.get(0).unwrap());
    assert_bf_equal(&bf_seq(5555 + PART_SIZE, 1), partitions.get(1).unwrap());
}

#[test]
fn multiple_partition_test() {
    let mut dls = Deadlines::new();
    dls.due[1] = bf_seq(0, 3 * PART_SIZE + 1);

    let partitions =
        compute_partitions_sector(&mut dls, PART_SIZE as u64, 1, &[0, 1, 2, 3]).unwrap();
    assert_eq!(4, partitions.len());

    assert_bf_equal(&bf_seq(0, PART_SIZE), partitions.get(0).unwrap());
    assert_bf_equal(
        &bf_seq(1 * PART_SIZE, PART_SIZE),
        partitions.get(1).unwrap(),
    );
    assert_bf_equal(
        &bf_seq(2 * PART_SIZE, PART_SIZE),
        partitions.get(2).unwrap(),
    );
    assert_bf_equal(&bf_seq(3 * PART_SIZE, 1), partitions.get(3).unwrap());
}

#[test]
fn numbered_partitions_test() {
    let mut dls = Deadlines::new();
    dls.due[1] = bf_seq(0, 3 * PART_SIZE + 1);
    dls.due[3] = bf_seq(3 * PART_SIZE + 1, 1);
    dls.due[5] = bf_seq(3 * PART_SIZE + 2, 2 * PART_SIZE);

    let mut partitions =
        compute_partitions_sector(&mut dls, PART_SIZE as u64, 1, &[0, 1, 2, 3]).unwrap();
    assert_eq!(4, partitions.len());

    partitions = compute_partitions_sector(&mut dls, PART_SIZE as u64, 3, &[4]).unwrap();
    assert_eq!(1, partitions.len());
    assert_bf_equal(&bf_seq(3 * PART_SIZE + 1, 1), partitions.get(0).unwrap());

    partitions = compute_partitions_sector(&mut dls, PART_SIZE as u64, 5, &[5, 6]).unwrap();
    assert_eq!(2, partitions.len());
    assert_bf_equal(
        &bf_seq(3 * PART_SIZE + 2, PART_SIZE),
        partitions.get(0).unwrap(),
    );
    assert_bf_equal(
        &bf_seq(3 * PART_SIZE + 2 + PART_SIZE, PART_SIZE),
        partitions.get(1).unwrap(),
    );
}

#[test]
fn numbered_partitions_should_err_test() {
    let mut dls = Deadlines::new();
    dls.due[1] = bf_seq(0, 3 * PART_SIZE + 1);
    dls.due[3] = bf_seq(3 * PART_SIZE + 1, 1);
    dls.due[5] = bf_seq(3 * PART_SIZE + 2, 2 * PART_SIZE);

    assert_eq!(
        compute_partitions_sector(&mut dls, PART_SIZE as u64, 1, &[4]).unwrap_err(),
        format!("invalid partition 4 at deadline 1 with first 0, count 4")
    );
    assert_eq!(
        compute_partitions_sector(&mut dls, PART_SIZE as u64, 2, &[4]).unwrap_err(),
        format!("invalid partition 4 at deadline 2 with first 4, count 0")
    );
    assert_eq!(
        compute_partitions_sector(&mut dls, PART_SIZE as u64, 3, &[0]).unwrap_err(),
        format!("invalid partition 0 at deadline 3 with first 4, count 1")
    );
    assert_eq!(
        compute_partitions_sector(&mut dls, PART_SIZE as u64, 3, &[3]).unwrap_err(),
        format!("invalid partition 3 at deadline 3 with first 4, count 1")
    );
    assert_eq!(
        compute_partitions_sector(&mut dls, PART_SIZE as u64, 3, &[5]).unwrap_err(),
        format!("invalid partition 5 at deadline 3 with first 4, count 1")
    );
    assert_eq!(
        compute_partitions_sector(&mut dls, PART_SIZE as u64, 4, &[5]).unwrap_err(),
        format!("invalid partition 5 at deadline 4 with first 5, count 0")
    );
    assert_eq!(
        compute_partitions_sector(&mut dls, PART_SIZE as u64, 5, &[0]).unwrap_err(),
        format!("invalid partition 0 at deadline 5 with first 5, count 2")
    );
    assert_eq!(
        compute_partitions_sector(&mut dls, PART_SIZE as u64, 5, &[7]).unwrap_err(),
        format!("invalid partition 7 at deadline 5 with first 5, count 2")
    );
}

const NEW_SECTOR_PART_SIZE: usize = 4;

#[test]
fn assign_new_sectors_test() {
    let mut deadlines = assign_sectors_setup(Deadlines::new(), &seq(0, 0));
    DeadlineBuilder::new(&[]).verify(&deadlines);

    deadlines = assign_sectors_setup(Deadlines::new(), &seq(0, 1));
    DeadlineBuilder::new(&[0, 1]).verify(&deadlines);

    deadlines = assign_sectors_setup(Deadlines::new(), &seq(0, 15));
    DeadlineBuilder::new(&[0, 4, 4, 4, 3]).verify(&deadlines);

    deadlines = assign_sectors_setup(
        Deadlines::new(),
        &seq(0, (WPOST_PERIOD_DEADLINES - 1) * NEW_SECTOR_PART_SIZE + 1),
    );
    DeadlineBuilder::new(&[])
        .add_to_all_from(1, NEW_SECTOR_PART_SIZE)
        .add_to(1, 1)
        .verify(&deadlines);
}

#[test]
fn incremental_assignment_test() {
    // Add one sector at a time.
    let mut deadlines = DeadlineBuilder::new(&[0, 1]).deadlines;
    deadlines = assign_sectors_setup(deadlines, &seq(1, 1));
    DeadlineBuilder::new(&[0, 2]).verify(&deadlines);
    deadlines = assign_sectors_setup(deadlines, &seq(2, 1));
    DeadlineBuilder::new(&[0, 3]).verify(&deadlines);
    deadlines = assign_sectors_setup(deadlines, &seq(3, 1));
    DeadlineBuilder::new(&[0, 4]).verify(&deadlines);
    deadlines = assign_sectors_setup(deadlines, &seq(4, 1));
    DeadlineBuilder::new(&[0, 4, 1]).verify(&deadlines);
    // Add one partition at a time.
    deadlines = Deadlines::new();
    deadlines = assign_sectors_setup(deadlines, &seq(0, 4));
    DeadlineBuilder::new(&[0, 4]).verify(&deadlines);
    deadlines = assign_sectors_setup(deadlines, &seq(4, 4));
    DeadlineBuilder::new(&[0, 4, 4]).verify(&deadlines);
    deadlines = assign_sectors_setup(deadlines, &seq(2 * 4, 4));
    DeadlineBuilder::new(&[0, 4, 4, 4]).verify(&deadlines);
    deadlines = assign_sectors_setup(deadlines, &seq(3 * 4, 4));
    DeadlineBuilder::new(&[0, 4, 4, 4, 4]).verify(&deadlines);
    // Add lots
    deadlines = Deadlines::new();
    deadlines = assign_sectors_setup(deadlines, &seq(0, 2 * NEW_SECTOR_PART_SIZE + 1));
    DeadlineBuilder::new(&[0, NEW_SECTOR_PART_SIZE, NEW_SECTOR_PART_SIZE, 1]).verify(&deadlines);
    deadlines = assign_sectors_setup(
        deadlines.clone(),
        &seq(2 * NEW_SECTOR_PART_SIZE + 1, NEW_SECTOR_PART_SIZE),
    );
    DeadlineBuilder::new(&[
        0,
        NEW_SECTOR_PART_SIZE,
        NEW_SECTOR_PART_SIZE,
        NEW_SECTOR_PART_SIZE,
        1,
    ])
    .verify(&deadlines);
}

#[test]
fn fill_partial_partitions_first_test() {
    let mut b = DeadlineBuilder::new(&[0, 4, 3, 1]);
    let mut deadlines = assign_sectors_setup(b.deadlines, &seq(b.next_sector_idx, 4));
    DeadlineBuilder::new(&[0, 4, 3, 1])
        .add_to(2, 1)
        .add_to(3, 3)
        .verify(&deadlines);

    b = DeadlineBuilder::new(&[0, 9, 8, 7, 4, 1]);
    deadlines = assign_sectors_setup(b.deadlines, &seq(b.next_sector_idx, 7));
    DeadlineBuilder::new(&[0, 9, 8, 7, 4, 1])
        .add_to(1, 3)
        .add_to(3, 1)
        .add_to(5, 3)
        .verify(&deadlines);

    // fill less full deadlines first
    let b = DeadlineBuilder::new(&[0, 12, 4, 4, 8]).add_to_all_from(5, 100);
    deadlines = assign_sectors_setup(b.deadlines, &seq(b.next_sector_idx, 20));
    DeadlineBuilder::new(&[0, 12, 4, 4, 8])
        .add_to_all_from(5, 100)
        .add_to(2, 4)
        .add_to(3, 4)
        .add_to(2, 4)
        .add_to(3, 4)
        .add_to(4, 4)
        .verify(&deadlines);
}

fn assign_sectors_setup(mut deadlines: Deadlines, sectors: &[usize]) -> Deadlines {
    assign_new_sectors(&mut deadlines, NEW_SECTOR_PART_SIZE, sectors).unwrap();
    return deadlines;
}

fn assert_bf_equal(expected: &BitField, actual: &BitField) {
    assert!(expected
        .bounded_iter(1 << 20)
        .unwrap()
        .eq(actual.bounded_iter(1 << 20).unwrap()));
}

fn assert_deadlines_equal(expected: &Deadlines, actual: &Deadlines) {
    for (i, v) in expected.due.iter().enumerate() {
        assert!(v
            .bounded_iter(1 << 20)
            .unwrap()
            .eq(actual.due[i].bounded_iter(1 << 20).unwrap()));
    }
}

fn build_deadlines(gen: &[usize]) -> Deadlines {
    // TODO switch deadline builder accordingly
    DeadlineBuilder::new(gen).deadlines
}

fn seq(first: usize, count: usize) -> Vec<usize> {
    let mut values: Vec<usize> = vec![0; count];
    for (i, val) in values.iter_mut().enumerate() {
        *val = first + i;
    }
    return values;
}

fn bf_seq(first: usize, count: usize) -> BitField {
    let values = seq(first, count);
    let bf: BitField = values.iter().copied().collect();
    bf
}

/// A builder for initialising a Deadlines with sectors assigned.
struct DeadlineBuilder {
    deadlines: Deadlines,
    next_sector_idx: usize,
}

impl DeadlineBuilder {
    fn new(counts: &[usize]) -> Self {
        DeadlineBuilder {
            deadlines: Deadlines::new(),
            next_sector_idx: 0,
        }
        .add_to_from(0, counts)
    }
    fn add_to(&mut self, idx: usize, count: usize) -> &mut Self {
        let nums = seq(self.next_sector_idx, count);
        self.next_sector_idx += count;
        self.deadlines.add_to_deadline(idx, &nums).unwrap();
        self
    }

    fn add_to_from(mut self, first: usize, counts: &[usize]) -> Self {
        for (i, c) in counts.into_iter().enumerate() {
            self.add_to(first + i, *c);
        }
        self
    }

    fn add_to_all(mut self, count: usize) -> Self {
        let len = self.deadlines.due.len();
        for i in 0..len {
            self.add_to(i, count);
        }
        self
    }

    fn add_to_all_from(mut self, first: usize, count: usize) -> Self {
        let mut i = first;
        while i < WPOST_PERIOD_DEADLINES {
            self.add_to(i, count);
            i += 1;
        }
        self
    }

    fn verify(&self, actual: &Deadlines) {
        assert_deadlines_equal(&self.deadlines, actual);
    }
}
