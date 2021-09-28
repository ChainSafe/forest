// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use bitfield::BitField;
use clock::ChainEpoch;
use std::{collections::HashMap, ops::AddAssign};

#[derive(Default)]
pub struct TerminationResult {
    /// Sectors maps epochs at which sectors expired, to bitfields of sector numbers.
    pub sectors: HashMap<ChainEpoch, BitField>,
    pub partitions_processed: u64,
    pub sectors_processed: u64,
}

impl AddAssign for TerminationResult {
    #[allow(clippy::suspicious_op_assign_impl)]
    fn add_assign(&mut self, rhs: Self) {
        self.partitions_processed += rhs.partitions_processed;
        self.sectors_processed += rhs.sectors_processed;

        for (epoch, new_sectors) in rhs.sectors {
            self.sectors
                .entry(epoch)
                .and_modify(|sectors| *sectors |= &new_sectors)
                .or_insert(new_sectors);
        }
    }
}

impl TerminationResult {
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns true if we're below the partition/sector limit. Returns false if
    /// we're at (or above) the limit.
    pub fn below_limit(&self, partition_limit: u64, sector_limit: u64) -> bool {
        self.partitions_processed < partition_limit && self.sectors_processed < sector_limit
    }

    pub fn is_empty(&self) -> bool {
        self.sectors_processed == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = (ChainEpoch, &BitField)> {
        let mut epochs: Vec<_> = self.sectors.iter().collect();
        epochs.sort_by_key(|&(&epoch, _)| epoch);
        epochs.into_iter().map(|(&i, bf)| (i, bf))
    }
}
