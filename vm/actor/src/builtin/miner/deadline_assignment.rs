// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{Deadline, SectorOnChainInfo};
use std::{cmp::Ordering, collections::BinaryHeap};

fn div_rounding_up(dividend: u64, divisor: u64) -> u64 {
    dividend / divisor + u64::from(dividend % divisor > 0)
}

struct DeadlineAssignmentInfo {
    index: usize,
    live_sectors: u64,
    total_sectors: u64,
}

impl DeadlineAssignmentInfo {
    fn partitions_after_assignment(&self, partition_size: u64) -> u64 {
        div_rounding_up(
            self.total_sectors + 1, // after assignment
            partition_size,
        )
    }

    fn compact_partitions_after_assignment(&self, partition_size: u64) -> u64 {
        div_rounding_up(
            self.live_sectors + 1, // after assignment
            partition_size,
        )
    }

    fn is_full_now(&self, partition_size: u64) -> bool {
        self.total_sectors % partition_size == 0
    }

    fn max_partitions_reached(&self, partition_size: u64, max_partitions: u64) -> bool {
        self.total_sectors >= partition_size * max_partitions
    }
}

fn cmp(a: &DeadlineAssignmentInfo, b: &DeadlineAssignmentInfo, partition_size: u64) -> Ordering {
    // When assigning partitions to deadlines, we're trying to optimize the
    // following:
    //
    // First, avoid increasing the maximum number of partitions in any
    // deadline, across all deadlines, after compaction. This would
    // necessitate buying a new GPU.
    //
    // Second, avoid forcing the miner to repeatedly compact partitions. A
    // miner would be "forced" to compact a partition when a the number of
    // partitions in any given deadline goes above the current maximum
    // number of partitions across all deadlines, and compacting that
    // deadline would then reduce the number of partitions, reducing the
    // maximum.
    //
    // At the moment, the only "forced" compaction happens when either:
    //
    // 1. Assignment of the sector into any deadline would force a
    //    compaction.
    // 2. The chosen deadline has at least one full partition's worth of
    //    terminated sectors and at least one fewer partition (after
    //    compaction) than any other deadline.
    //
    // Third, we attempt to assign "runs" of sectors to the same partition
    // to reduce the size of the bitfields.
    //
    // Finally, we try to balance the number of sectors (thus partitions)
    // assigned to any given deadline over time.

    // Summary:
    //
    // 1. Assign to the deadline that will have the _least_ number of
    //    post-compaction partitions (after sector assignment).
    // 2. Assign to the deadline that will have the _least_ number of
    //    pre-compaction partitions (after sector assignment).
    // 3. Assign to a deadline with a non-full partition.
    //    - If both have non-full partitions, assign to the most full one (stable assortment).
    // 4. Assign to the deadline with the least number of live sectors.
    // 5. Assign sectors to the deadline with the lowest index first.

    // If one deadline would end up with fewer partitions (after
    // compacting), assign to that one. This ensures we keep the maximum
    // number of partitions in any given deadline to a minimum.
    //
    // Technically, this could increase the maximum number of partitions
    // before compaction. However, that can only happen if the deadline in
    // question could save an entire partition by compacting. At that point,
    // the miner should compact the deadline.
    a.compact_partitions_after_assignment(partition_size)
        .cmp(&b.compact_partitions_after_assignment(partition_size))
        .then_with(|| {
            // If, after assignment, neither deadline would have fewer
            // post-compaction partitions, assign to the deadline with the fewest
            // pre-compaction partitions (after assignment). This will put off
            // compaction as long as possible.
            a.partitions_after_assignment(partition_size)
                .cmp(&b.partitions_after_assignment(partition_size))
        })
        .then_with(|| {
            // Ok, we'll end up with the same number of partitions any which way we
            // go. Try to fill up a partition instead of opening a new one.
            a.is_full_now(partition_size)
                .cmp(&b.is_full_now(partition_size))
        })
        .then_with(|| {
            // Either we have two open partitions, or neither deadline has an open
            // partition.

            // If we have two open partitions, fill the deadline with the most-full
            // open partition. This helps us assign runs of sequential sectors into
            // the same partition.
            if !a.is_full_now(partition_size) && !b.is_full_now(partition_size) {
                a.total_sectors.cmp(&b.total_sectors).reverse()
            } else {
                Ordering::Equal
            }
        })
        .then_with(|| {
            // Otherwise, assign to the deadline with the least live sectors. This
            // will break the tie in one of the two immediately preceding
            // conditions.
            a.live_sectors.cmp(&b.live_sectors)
        })
        .then_with(|| {
            // Finally, fall back on the deadline index.
            a.index.cmp(&b.index)
        })
}

// Assigns partitions to deadlines, first filling partial partitions, then
// adding new partitions to deadlines with the fewest live sectors.
pub fn assign_deadlines(
    max_partitions: u64,
    partition_size: u64,
    deadlines: &[Option<Deadline>],
    sectors: Vec<SectorOnChainInfo>,
) -> Result<Vec<Vec<SectorOnChainInfo>>, String> {
    struct Entry {
        partition_size: u64,
        info: DeadlineAssignmentInfo,
    }

    impl PartialEq for Entry {
        fn eq(&self, other: &Self) -> bool {
            self.cmp(other) == Ordering::Equal
        }
    }

    impl Eq for Entry {}

    impl PartialOrd for Entry {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for Entry {
        fn cmp(&self, other: &Self) -> Ordering {
            // we're using a max heap instead of a min heap, so we need to reverse the ordering
            cmp(&self.info, &other.info, self.partition_size).reverse()
        }
    }

    let mut heap: BinaryHeap<Entry> = deadlines
        .iter()
        .enumerate()
        .filter_map(|(index, deadline)| deadline.as_ref().map(|dl| (index, dl)))
        .map(|(index, deadline)| Entry {
            partition_size,
            info: DeadlineAssignmentInfo {
                index,
                live_sectors: deadline.live_sectors,
                total_sectors: deadline.total_sectors,
            },
        })
        .collect();

    assert!(!heap.is_empty());

    let mut changes = vec![Vec::new(); super::WPOST_PERIOD_DEADLINES as usize];

    for sector in sectors {
        let info = &mut heap.peek_mut().unwrap().info;

        if info.max_partitions_reached(partition_size, max_partitions) {
            return Err(format!(
                "max partitions limit {} reached for all deadlines",
                max_partitions
            ));
        }

        changes[info.index].push(sector);
        info.live_sectors += 1;
        info.total_sectors += 1;
    }

    Ok(changes)
}
