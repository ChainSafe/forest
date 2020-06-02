// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(unused_variables)]
#![allow(dead_code)]
use super::policy::*;
use crate::OptionalEpoch;
use clock::ChainEpoch;
use encoding::tuple::*;
use std::collections::HashMap;
use vm::Randomness;
use bitfield::BitField;
/// Deadline calculations with respect to a current epoch.
/// "Deadline" refers to the window during which proofs may be submitted.
/// Windows are non-overlapping ranges [Open, Close), but the challenge epoch for a window occurs before
/// the window opens.
pub struct DeadlineInfo {
    /// Epoch at which this info was calculated.
    current_epoch: ChainEpoch,
    /// First epoch of the proving period (<= CurrentEpoch).
    period_start: ChainEpoch,
    /// Current deadline index, in [0..WPoStProvingPeriodDeadlines).
    index: u64,
    /// First epoch from which a proof may be submitted, inclusive (>= CurrentEpoch).
    open: ChainEpoch,
    /// First epoch from which a proof may no longer be submitted, exclusive (>= Open).
    close: ChainEpoch,
    /// Epoch at which to sample the chain for challenge (< Open).
    challenge: ChainEpoch,
    /// First epoch at which a fault declaration is rejected (< Open).
    fault_cutoff: ChainEpoch,
}
impl DeadlineInfo {
    fn new(period_start: ChainEpoch, deadline_idx: u64, current_epoch: ChainEpoch) -> Self {
        if deadline_idx < WPOST_PERIOD_DEADLINES {
            let deadline_open = period_start + deadline_idx + WPOST_CHALLENGE_WINDOW;
            Self {
                current_epoch,
                period_start,
                index: deadline_idx,
                open: deadline_open,
                close: deadline_open + WPOST_CHALLENGE_WINDOW,
                challenge: deadline_open - WPOST_CHALLENGE_LOOKBACK,
                fault_cutoff: deadline_open - FAULT_DECLARATION_CUTOFF,
            }
        } else {
            let after_last_deadline = period_start + WPOST_PROVING_PERIOD;
            Self {
                current_epoch,
                period_start,
                index: deadline_idx,
                open: after_last_deadline,
                close: after_last_deadline,
                challenge: after_last_deadline,
                fault_cutoff: 0,
            }
        }
    }
    /// Whether the proving period has begun.
    fn period_start(&self) -> bool {
        self.current_epoch >= self.period_start
    }
    /// Whether the proving period has elapsed.
    fn period_elapsed(&self) -> bool {
        self.current_epoch >= self.next_period_start()
    }
    /// Whether the current deadline is currently open.
    fn is_open(&self) -> bool {
        self.current_epoch >= self.open && self.current_epoch < self.close
    }
    /// Whether the current deadline has already closed.
    fn has_elapsed(&self) -> bool {
        self.current_epoch >= self.close
    }
    /// Whether the deadline's fault cutoff has passed.
    fn fault_cutoff_passed(&self) -> bool {
        self.current_epoch >= self.fault_cutoff
    }
    /// The last epoch in the proving period.
    fn period_end(&self) -> ChainEpoch {
        self.period_start + WPOST_PROVING_PERIOD - 1
    }
    /// The first epoch in the next proving period.
    fn next_period_start(&self) -> ChainEpoch {
        self.period_start + WPOST_PROVING_PERIOD
    }
}
/// Calculates the deadline at some epoch for a proving period and returns the deadline-related calculations.
pub fn compute_proving_period_deadline(
    period_start: OptionalEpoch,
    current_epoch: ChainEpoch,
) -> Option<DeadlineInfo> {
    let start_period = match period_start.0 {
        Some(start_period) => start_period,
        None => return None,
    };
    let period_progress = current_epoch - start_period;
    if period_progress >= WPOST_PROVING_PERIOD {
        // Proving period has completely elapsed.
        return Some(DeadlineInfo::new(
            start_period,
            WPOST_PERIOD_DEADLINES,
            current_epoch,
        ));
    }
    let period_progress = current_epoch - start_period;
    if period_progress >= WPOST_PROVING_PERIOD {
        return Some(DeadlineInfo::new(
            start_period,
            WPOST_PERIOD_DEADLINES,
            current_epoch,
        ));
    }
    let deadline_idx = period_progress / WPOST_CHALLENGE_WINDOW;
    Some(DeadlineInfo::new(start_period, deadline_idx, current_epoch))
}
/// Computes the first partition index and number of sectors for a deadline.
/// Partitions are numbered globally for the miner, not per-deadline.
/// If the deadline has no sectors, the first partition index is the index that a partition at that deadline would
/// have, if non-empty (and sectorCount is zero).
fn parititions_for_deadline(
    d: &Deadlines,
    partition_size: u64,
    deadline_idx: u64,
) -> Result<(u64, u64), String> {
    if deadline_idx >= WPOST_PERIOD_DEADLINES {
        return Err(format!(
            "invalid deadline index {} for {} deadlines",
            deadline_idx, WPOST_PERIOD_DEADLINES
        ));
    }
    let mut partition_count_so_far: u64 = 0;
    for i in 0..WPOST_PERIOD_DEADLINES {
        let (partition_count, sector_count) = deadline_count(&d, partition_size, i)?;
        if i == deadline_idx {
            return Ok((partition_count_so_far, sector_count));
        }
        partition_count_so_far += partition_count;
    }
    Ok((0, 0))
}
/// Counts the partitions (including up to one partial) and sectors at a deadline.
pub fn deadline_count(
    d: &Deadlines,
    partition_size: u64,
    deadline_idx: u64,
) -> Result<(u64, u64), String> {
    if deadline_idx >= WPOST_PERIOD_DEADLINES {
        return Err(format!(
            "invalid deadline index {} for {} deadlines",
            deadline_idx, WPOST_PERIOD_DEADLINES
        ));
    }
    // TODO bit ops
    //let sector_count = d.due[deadline_idx as usize].count();
    let sector_count: u64 = 2;
    let mut paritition_count = sector_count / partition_size;
    if sector_count % partition_size != 0 {
        paritition_count += 1;
    };
    Ok((paritition_count, sector_count))
}
/// Computes a bitfield of the sector numbers included in a sequence of partitions due at some deadline.
/// Fails if any partition is not due at the provided deadline.
pub fn compute_partitions_sector(
    d: &Deadlines,
    partition_size: u64,
    deadline_idx: u64,
    partitions: &[u64],
) -> Result<BitField, String> {
    let (deadline_first_partition, deadline_sector_count) =
        parititions_for_deadline(d, partition_size, deadline_idx)?;
    let deadline_partition_count = (deadline_sector_count + partition_size - 1) / partition_size;
    // Work out which sector numbers the partitions correspond to.
    let deadline_sectors = d.due[deadline_idx as usize];
    let partitions_sectors = BitField::default();
    for p_idx in partitions {
        if p_idx < &deadline_first_partition
            || p_idx >= &(deadline_first_partition + deadline_partition_count)
        {
            return Err(format!(
                "invalid partition {} at deadline {} with first {}, count {}",
                p_idx, deadline_idx, deadline_first_partition, deadline_partition_count
            ));
        }
        // Slice out the sectors corresponding to this partition from the deadline's sector bitfield.
        let sector_offset = (p_idx - deadline_first_partition) * partition_size;
        let sector_count = std::cmp::min(partition_size, deadline_sector_count - sector_offset);
        let partition_sectors = deadline_sectors
            .splice(sector_count, sector_offset)
            .collect::<BitField>();
        partitions_sectors.append(partition_sectors);
    }
    Ok(partitions_sectors)
}
/// Assigns a sequence of sector numbers to deadlines by:
/// - filling any non-full partitions, in round-robin order across the deadlines
/// - repeatedly adding a new partition to the deadline with the fewest partitions
/// When multiple partitions share the minimal sector count, one is chosen at random (from a seed).
pub fn assign_new_sectors(
    deadlines: &Deadlines,
    partition_size: u64,
    new_sectors: &[u64],
    _seed: Randomness,
) -> Result<(), String> {
    let mut next_new_sector: usize = 0;
    let new_sector_length = new_sectors.len();
    // Assigns up to `count` sectors to `deadline` and advances `nextNewSector`.
    let mut assign_to_deadline = |count: usize, deadline: u64| -> Result<(), String> {
        let count_to_add = std::cmp::min(count, new_sector_length - next_new_sector.clone());
        let limit = next_new_sector.clone() + count_to_add;
        let sectors_to_add = &new_sectors[next_new_sector..limit];
        deadlines.add_to_deadline(deadline, sectors_to_add)?;
        next_new_sector += count_to_add;
        Ok(())
    };
    // Iterate deadlines and fill any partial partitions. There's no great advantage to filling more- or less-
    // full ones first, so they're filled in sequence order.
    // Meanwhile, record the partition count at each deadline.
    let deadline_partitions_counts: HashMap<u64, u64> = HashMap::default();
    let mut i = 0;
    while i < WPOST_PERIOD_DEADLINES && next_new_sector.clone() < new_sector_length {
        let (partition_count, sector_count) = deadline_count(&deadlines, partition_size, i)?;
        deadline_partitions_counts[&i] = partition_count;
        let gap = partition_size - (sector_count % partition_size);
        if gap != partition_size {
            assign_to_deadline(gap as usize, i)?;
        }
        i += 1;
    }
    // While there remain new sectors to assign, fill a new partition at each deadline in round-robin fashion.
    // TODO WPOST (follow-up): fill less-full deadlines first, randomize when equally full.
    let mut target_deadline: u64 = 0;
    while next_new_sector < new_sectors.len() {
        assign_to_deadline(partition_size as usize, target_deadline)?;
        target_deadline = (target_deadline + 1) % WPOST_PERIOD_DEADLINES;
    }
    Ok(())
}