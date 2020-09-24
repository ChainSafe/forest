// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::policy::*;
use super::{Deadlines, Partition, QuantSpec};
use clock::ChainEpoch;
use fil_types::SectorNumber;
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use serde::{Deserialize, Serialize};
use std::error::Error as StdError;

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
    pub index: u64,
    /// First epoch from which a proof may be submitted (>= CurrentEpoch).
    pub open: ChainEpoch,
    /// First epoch from which a proof may no longer be submitted (>= Open).
    pub close: ChainEpoch,
    /// Epoch at which to sample the chain for challenge (< Open).
    pub challenge: ChainEpoch,
    /// First epoch at which a fault declaration is rejected (< Open).
    pub fault_cutoff: ChainEpoch,
}

impl DeadlineInfo {
    pub fn new(period_start: ChainEpoch, deadline_idx: u64, current_epoch: ChainEpoch) -> Self {
        if deadline_idx < WPOST_PERIOD_DEADLINES as u64 {
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

    /// The last epoch in the proving period.
    pub fn period_end(&self) -> ChainEpoch {
        self.period_start + WPOST_PROVING_PERIOD - 1
    }

    /// The first epoch in the next proving period.
    pub fn next_period_start(&self) -> ChainEpoch {
        self.period_start + WPOST_PROVING_PERIOD
    }

    /// Whether the current deadline is currently open.
    pub fn is_open(&self) -> bool {
        self.current_epoch >= self.open && self.current_epoch < self.close
    }

    /// Whether the current deadline has already closed.
    pub fn has_elapsed(&self) -> bool {
        self.current_epoch >= self.close
    }

    /// The last epoch during which a proof may be submitted.
    pub fn last(&self) -> ChainEpoch {
        self.close - 1
    }

    /// Epoch at which the subsequent deadline opens.
    pub fn next_open(&self) -> ChainEpoch {
        self.close
    }

    /// Whether the deadline's fault cutoff has passed.
    pub fn fault_cutoff_passed(&self) -> bool {
        self.current_epoch >= self.fault_cutoff
    }

    /// Returns the next instance of this deadline that has not yet elapsed.
    pub fn next_not_elapsed(self) -> Self {
        std::iter::successors(Some(self), |info| {
            Some(Self::new(
                info.next_period_start(),
                info.index,
                info.current_epoch,
            ))
        })
        .find(|info| !info.has_elapsed())
        .unwrap() // the iterator is infinite, so `find` won't ever return `None`
    }

    pub fn quant_spec(&self) -> QuantSpec {
        QuantSpec {
            unit: WPOST_PROVING_PERIOD,
            offset: self.last(),
        }
    }
}

impl Deadlines {
    /// Returns the deadline and partition index for a sector number.
    /// Returns an error if the sector number is not tracked by `self`.
    pub fn find_sector<BS: BlockStore>(
        &self,
        store: &BS,
        sector_number: SectorNumber,
    ) -> Result<(u64, u64), Box<dyn StdError>> {
        for i in 0..self.due.len() {
            let deadline_idx = i as u64;
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
    deadline_idx: u64,
    current_epoch: ChainEpoch,
) -> bool {
    // Get the next non-elapsed deadline (i.e., the next time we care about
    // mutations to the deadline).
    let deadline_info =
        DeadlineInfo::new(proving_period_start, deadline_idx, current_epoch).next_not_elapsed();

    // Ensure that the current epoch is at least one challenge window before
    // that deadline opens.
    current_epoch < deadline_info.open - WPOST_CHALLENGE_WINDOW
}
