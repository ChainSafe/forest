// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::policy::*;
use super::state::Deadlines;
use bitfield::BitField;
use clock::ChainEpoch;
use serde::{Deserialize, Serialize};

/// Deadline calculations with respect to a current epoch.
/// "Deadline" refers to the window during which proofs may be submitted.
/// Windows are non-overlapping ranges [Open, Close), but the challenge epoch for a window occurs before
/// the window opens.
#[derive(Default, Debug, Serialize, Deserialize, PartialEq)]
pub struct DeadlineInfo {
    /// Epoch at which this info was calculated.
    pub current_epoch: ChainEpoch,
    /// First epoch of the proving period (<= CurrentEpoch).
    pub period_start: ChainEpoch,
    /// Current deadline index, in [0..WPoStProvingPeriodDeadlines).
    pub index: usize,
    /// First epoch from which a proof may be submitted, inclusive (>= CurrentEpoch).
    pub open: ChainEpoch,
    /// First epoch from which a proof may no longer be submitted, exclusive (>= Open).
    pub close: ChainEpoch,
    /// Epoch at which to sample the chain for challenge (< Open).
    pub challenge: ChainEpoch,
    /// First epoch at which a fault declaration is rejected (< Open).
    pub fault_cutoff: ChainEpoch,
}

impl DeadlineInfo {
    pub fn new(period_start: ChainEpoch, deadline_idx: usize, current_epoch: ChainEpoch) -> Self {
        if deadline_idx < WPOST_PERIOD_DEADLINES {
            let deadline_open = period_start + (deadline_idx as i64 * WPOST_CHALLENGE_WINDOW);
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
    pub fn period_started(&self) -> bool {
        self.current_epoch >= self.period_start
    }
    /// Whether the proving period has elapsed.
    pub fn period_elapsed(&self) -> bool {
        self.current_epoch >= self.next_period_start()
    }
    /// Whether the current deadline is currently open.
    pub fn is_open(&self) -> bool {
        self.current_epoch >= self.open && self.current_epoch < self.close
    }
    /// Whether the current deadline has already closed.
    pub fn has_elapsed(&self) -> bool {
        self.current_epoch >= self.close
    }
    /// Whether the deadline's fault cutoff has passed.
    pub fn fault_cutoff_passed(&self) -> bool {
        self.current_epoch >= self.fault_cutoff
    }
    /// The last epoch in the proving period.
    pub fn period_end(&self) -> ChainEpoch {
        self.period_start + WPOST_PROVING_PERIOD - 1
    }
    /// The first epoch in the next proving period.
    pub fn next_period_start(&self) -> ChainEpoch {
        self.period_start + WPOST_PROVING_PERIOD
    }
}
/// Calculates the deadline at some epoch for a proving period and returns the deadline-related calculations.
pub fn compute_proving_period_deadline(
    period_start: ChainEpoch,
    current_epoch: ChainEpoch,
) -> DeadlineInfo {
    let period_progress = current_epoch - period_start;
    if period_progress >= WPOST_PROVING_PERIOD {
        // Proving period has completely elapsed.
        return DeadlineInfo::new(period_start, WPOST_PERIOD_DEADLINES, current_epoch);
    }
    let deadline_idx = if period_progress < 0 {
        0
    } else {
        period_progress / WPOST_CHALLENGE_WINDOW
    };
    DeadlineInfo::new(period_start, deadline_idx as usize, current_epoch)
}
/// Computes the first partition index and number of sectors for a deadline.
/// Partitions are numbered globally for the miner, not per-deadline.
/// If the deadline has no sectors, the first partition index is the index that a partition at that deadline would
/// have, if non-empty (and sectorCount is zero).
pub fn partitions_for_deadline(
    d: &mut Deadlines,
    partition_size: usize,
    deadline_idx: usize,
) -> Result<(u64, u64), String> {
    if deadline_idx >= WPOST_PERIOD_DEADLINES {
        return Err(format!(
            "invalid deadline index {} for {} deadlines",
            deadline_idx, WPOST_PERIOD_DEADLINES
        ));
    }
    let mut partition_count_so_far: u64 = 0;
    for i in 0..WPOST_PERIOD_DEADLINES {
        let (partition_count, sector_count) = deadline_count(d, partition_size, i)?;
        if i == deadline_idx {
            return Ok((partition_count_so_far, sector_count as u64));
        }
        partition_count_so_far += partition_count as u64;
    }
    Ok((0, 0))
}
/// Counts the partitions (including up to one partial) and sectors at a deadline.
pub fn deadline_count(
    d: &mut Deadlines,
    partition_size: usize,
    deadline_idx: usize,
) -> Result<(usize, usize), String> {
    if deadline_idx >= WPOST_PERIOD_DEADLINES {
        return Err(format!(
            "invalid deadline index {} for {} deadlines",
            deadline_idx, WPOST_PERIOD_DEADLINES
        ));
    }

    let sector_count = d.due[deadline_idx].len();
    let partition_count = (sector_count + partition_size - 1) / partition_size;
    Ok((partition_count, sector_count))
}
/// Computes a bitfield of the sector numbers included in a sequence of partitions due at some deadline.
/// Fails if any partition is not due at the provided deadline.
pub fn compute_partitions_sector(
    d: &mut Deadlines,
    partition_size: u64,
    deadline_idx: usize,
    partitions: &[u64],
) -> Result<Vec<BitField>, String> {
    let (deadline_first_partition, deadline_sector_count) =
        partitions_for_deadline(d, partition_size as usize, deadline_idx)?;
    let deadline_partition_count = (deadline_sector_count + partition_size - 1) / partition_size;
    // Work out which sector numbers the partitions correspond to.
    let deadline_sectors = d
        .due
        .get(deadline_idx)
        .ok_or(format!("unable to find deadline: {}", deadline_idx))?;
    let partitions_sectors = partitions
        .iter()
        .map(|&p_idx| {
            if p_idx < deadline_first_partition
                || p_idx >= deadline_first_partition + deadline_partition_count
            {
                return Err(format!(
                    "invalid partition {} at deadline {} with first {}, count {}",
                    p_idx, deadline_idx, deadline_first_partition, deadline_partition_count
                ));
            }
            // Slice out the sectors corresponding to this partition from the deadline's sector bitfield.
            let sector_offset = (p_idx - deadline_first_partition) * partition_size;
            let sector_count = std::cmp::min(partition_size, deadline_sector_count - sector_offset);
            let partition_sectors =
                deadline_sectors.slice(sector_offset as usize, sector_count as usize)?;
            Ok(partition_sectors)
        })
        .collect::<Result<_, _>>()?;
    Ok(partitions_sectors)
}
/// Assigns a sequence of sector numbers to deadlines by:
/// - filling any non-full partitions, in round-robin order across the deadlines
/// - repeatedly adding a new partition to the deadline with the fewest partitions
/// When multiple partitions share the minimal sector count, one is chosen at random (from a seed).
pub fn assign_new_sectors(
    deadlines: &mut Deadlines,
    partition_size: usize,
    new_sectors: &[usize],
) -> Result<(), String> {
    let mut next_new_sector: usize = 0;
    // The first deadline is left empty since it's more difficult for a miner to orchestrate proofs.
    // The set of sectors due at the deadline isn't known until the proving period actually starts and any
    // new sectors are assigned to it (here).
    // Practically, a miner must also wait for some probabilistic finality after that before beginning proof
    // calculations.
    // It's left empty so a miner has at least one challenge duration to prepare for proving after new sectors
    // are assigned.
    let first_assignable_deadline: usize = 1;

    // Assigns up to `count` sectors to `deadline` and advances `nextNewSector`.
    let assign_to_deadline = |count: usize,
                              deadline: usize,
                              next_new_sector: &mut usize,
                              deadlines: &mut Deadlines|
     -> Result<(), String> {
        let count_to_add = std::cmp::min(count, new_sectors.len() - *next_new_sector);
        let limit = *next_new_sector + count_to_add;
        let sectors_to_add = &new_sectors[*next_new_sector..limit];
        deadlines.add_to_deadline(deadline, sectors_to_add)?;
        *next_new_sector += count_to_add;
        Ok(())
    };
    // Iterate deadlines and fill any partial partitions. There's no great advantage to filling more- or less-
    // full ones first, so they're filled in sequence order.
    // Meanwhile, record the partition count at each deadline.
    let mut deadline_partitions_counts = [0usize; WPOST_PERIOD_DEADLINES];
    let mut i: usize = 0;
    while i < WPOST_PERIOD_DEADLINES && next_new_sector < new_sectors.len() {
        if i < first_assignable_deadline {
            // Mark unassignable deadlines as "full" so nothing more will be assigned.
            deadline_partitions_counts[i] = u64::max_value() as usize;
            i += 1;
            continue;
        }
        let (partition_count, sector_count) = deadline_count(deadlines, partition_size, i)?;
        deadline_partitions_counts[i] = partition_count;
        let gap = partition_size - (sector_count % partition_size);
        if gap != partition_size {
            assign_to_deadline(gap, i, &mut next_new_sector, deadlines)?;
        }
        i += 1;
    }

    // While there remain new sectors to assign, fill a new partition in one of the deadlines that is least full.
    // Do this by maintaining a slice of deadline indexes sorted by partition count.
    // Shuffling this slice to re-sort as weights change is O(n^2).
    // For a large number of partitions, a heap would be the way to do this in O(n*log n), but for small numbers
    // is probably overkill.
    // A miner onboarding a monumental 1EiB of 32GiB sectors uniformly throughout a year will fill 40 partitions
    // per proving period (40^2=1600). With 64GiB sectors, half that (20^2=400).
    // TODO: randomize assignment among equally-full deadlines https://github.com/filecoin-project/specs-actors/issues/432

    let mut dl_idxs = [0usize; WPOST_PERIOD_DEADLINES];
    for (i, v) in (0..).zip(dl_idxs.iter_mut()) {
        *v = i;
    }

    let sort_deadlines = |deadl: &[usize; WPOST_PERIOD_DEADLINES],
                          dl_idxs: &mut [usize; WPOST_PERIOD_DEADLINES]| {
        dl_idxs.sort_by_cached_key(|&i| (deadl[i], i));
    };
    sort_deadlines(&deadline_partitions_counts, &mut dl_idxs);
    while next_new_sector < new_sectors.len() {
        // Assign a full partition to the least-full deadline.
        let target_deadline = dl_idxs[0];
        assign_to_deadline(
            partition_size as usize,
            target_deadline,
            &mut next_new_sector,
            deadlines,
        )?;

        deadline_partitions_counts[target_deadline] += 1;
        // Re-sort the queue.
        // Only the first element has changed, the remainder is still sorted, so with an insertion-sort under
        // the hood this will be linear.
        sort_deadlines(&deadline_partitions_counts, &mut dl_idxs);
    }
    Ok(())
}
