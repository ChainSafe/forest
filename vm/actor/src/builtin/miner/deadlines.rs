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
