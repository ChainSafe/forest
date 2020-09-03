use bitfield::BitField;
use clock::ChainEpoch;
use std::collections::HashMap;

#[derive(Default)]
pub struct TerminationResult {
    /// Sectors maps epochs at which sectors expired, to bitfields of sector numbers.
    pub sectors: HashMap<ChainEpoch, BitField>,
    pub partitions_processed: u64,
    pub sectors_processed: u64,
}

impl TerminationResult {
    pub fn add(&mut self, new_result: TerminationResult) {
        self.partitions_processed += new_result.partitions_processed;
        self.sectors_processed += new_result.sectors_processed;

        for (epoch, new_sectors) in new_result.sectors {
            self.sectors
                .entry(epoch)
                .and_modify(|sectors| *sectors |= &new_sectors)
                .or_insert(new_sectors);
        }
    }

    /// Returns true if we're below the partition/sector limit. Returns false if
    /// we're at (or above) the limit.
    pub fn below_limit(&self, partition_limit: u64, sector_limit: u64) -> bool {
        self.partitions_processed < partition_limit && self.sectors_processed < sector_limit
    }

    pub fn is_empty(&self) -> bool {
        self.sectors_processed == 0
    }

    pub fn for_each(&self, mut f: impl FnMut(ChainEpoch, &BitField) -> ()) {
        let mut epochs: Vec<_> = self.sectors.iter().collect();
        epochs.sort_by_key(|&(&epoch, _)| epoch);

        for (&epoch, sectors) in epochs {
            f(epoch, sectors)
        }
    }
}
