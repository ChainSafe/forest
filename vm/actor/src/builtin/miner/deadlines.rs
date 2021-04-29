// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::policy::*;
use super::{Deadlines, Partition};
use clock::ChainEpoch;
use fil_types::{
    deadlines::{DeadlineInfo, QuantSpec},
    SectorNumber,
};
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use std::error::Error as StdError;

pub fn new_deadline_info(
    proving_period_start: ChainEpoch,
    deadline_idx: usize,
    current_epoch: ChainEpoch,
) -> DeadlineInfo {
    DeadlineInfo::new(
        proving_period_start,
        deadline_idx as u64,
        current_epoch,
        WPOST_PERIOD_DEADLINES as u64,
        WPOST_PROVING_PERIOD,
        WPOST_CHALLENGE_WINDOW,
        WPOST_CHALLENGE_LOOKBACK,
        FAULT_DECLARATION_CUTOFF,
    )
}

impl Deadlines {
    /// Returns the deadline and partition index for a sector number.
    /// Returns an error if the sector number is not tracked by `self`.
    pub fn find_sector<BS: BlockStore>(
        &self,
        store: &BS,
        sector_number: SectorNumber,
    ) -> Result<(usize, usize), Box<dyn StdError>> {
        for i in 0..self.due.len() {
            let deadline_idx = i;
            let deadline = self.load_deadline(store, deadline_idx)?;
            let partitions = Amt::<Partition, _>::load(&deadline.partitions, store)?;

            let mut partition_idx = None;

            partitions.for_each_while(|i, partition| {
                if partition.sectors.get(sector_number as usize) {
                    partition_idx = Some(i);
                    Ok(false)
                } else {
                    Ok(true)
                }
            })?;

            if let Some(partition_idx) = partition_idx {
                return Ok((deadline_idx, partition_idx));
            }
        }

        Err(format!("sector {} not due at any deadline", sector_number).into())
    }
}

/// Returns true if the deadline at the given index is currently mutable.
pub fn deadline_is_mutable(
    proving_period_start: ChainEpoch,
    deadline_idx: usize,
    current_epoch: ChainEpoch,
) -> bool {
    // Get the next non-elapsed deadline (i.e., the next time we care about
    // mutations to the deadline).
    let deadline_info =
        new_deadline_info(proving_period_start, deadline_idx, current_epoch).next_not_elapsed();

    // Ensure that the current epoch is at least one challenge window before
    // that deadline opens.
    current_epoch < deadline_info.open - WPOST_CHALLENGE_WINDOW
}

pub fn quant_spec_for_deadline(di: &DeadlineInfo) -> QuantSpec {
    QuantSpec {
        unit: WPOST_PROVING_PERIOD,
        offset: di.last(),
    }
}

// Returns true if optimistically accepted posts submitted to the given deadline
// may be disputed. Specifically, this ensures that:
//
// 1. Optimistic PoSts may not be disputed while the challenge window is open.
// 2. Optimistic PoSts may not be disputed after the miner could have compacted the deadline.
pub fn deadline_available_for_optimistic_post_dispute(
    proving_period_start: ChainEpoch,
    deadline_idx: usize,
    current_epoch: ChainEpoch,
) -> bool {
    if proving_period_start > current_epoch {
        return false;
    }
    let dl_info =
        new_deadline_info(proving_period_start, deadline_idx, current_epoch).next_not_elapsed();

    !dl_info.is_open()
        && current_epoch < (dl_info.close - WPOST_PROVING_PERIOD) + WPOST_DISPUTE_WINDOW
}

// Returns true if the given deadline may compacted in the current epoch.
// Deadlines may not be compacted when:
//
// 1. The deadline is currently being challenged.
// 2. The deadline is to be challenged next.
// 3. Optimistically accepted posts from the deadline's last challenge window
//    can currently be disputed.
pub fn deadline_available_for_compaction(
    proving_period_start: ChainEpoch,
    deadline_idx: usize,
    current_epoch: ChainEpoch,
) -> bool {
    deadline_is_mutable(proving_period_start, deadline_idx, current_epoch)
        && !deadline_available_for_optimistic_post_dispute(
            proving_period_start,
            deadline_idx,
            current_epoch,
        )
}

// Determine current period start and deadline index directly from current epoch and
// the offset implied by the proving period. This works correctly even for the state
// of a miner actor without an active deadline cron
pub fn new_deadline_info_from_offset_and_epoch(
    period_start_seed: ChainEpoch,
    current_epoch: ChainEpoch,
) -> DeadlineInfo {
    let q = QuantSpec {
        unit: WPOST_PROVING_PERIOD,
        offset: period_start_seed,
    };
    let current_period_start = q.quantize_down(current_epoch);
    let current_deadline_idx = ((current_epoch - current_period_start) / WPOST_CHALLENGE_WINDOW)
        as u64
        % WPOST_PERIOD_DEADLINES;
    new_deadline_info(
        current_period_start,
        current_deadline_idx as usize,
        current_epoch,
    )
}
