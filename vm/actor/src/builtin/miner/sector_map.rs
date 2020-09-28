// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::WPOST_PERIOD_DEADLINES;
use bitfield::BitField;
use std::collections::HashMap;

/// Maps deadlines to partition maps.
#[derive(Default)]
pub struct DeadlineSectorMap(HashMap<u64, PartitionSectorMap>);

impl DeadlineSectorMap {
    pub fn new() -> Self {
        Default::default()
    }

    /// Check validates all bitfields and counts the number of partitions & sectors
    /// contained within the map, and returns an error if they exceed the given
    /// maximums.
    pub fn check(&self, max_partitions: u64, max_sectors: u64) -> Result<(), String> {
        let (partition_count, sector_count) = self
            .count()
            .map_err(|e| format!("failed to count sectors: {:?}", e))?;

        if partition_count > max_partitions {
            return Err(format!(
                "too many partitions {}, max {}",
                partition_count, max_partitions
            ));
        }

        if sector_count > max_sectors {
            return Err(format!(
                "too many sectors {}, max {}",
                sector_count, max_sectors
            ));
        }

        Ok(())
    }

    /// Counts the number of partitions & sectors within the map.
    pub fn count(&self) -> Result<(/* partitions */ u64, /* sectors */ u64), String> {
        self.0.iter().try_fold(
            (0_u64, 0_u64),
            |(partitions, sectors), (deadline_idx, pm)| {
                let (partition_count, sector_count) = pm
                    .count()
                    .map_err(|e| format!("when counting deadline {}: {:?}", deadline_idx, e))?;
                Ok((
                    partitions
                        .checked_add(partition_count)
                        .ok_or_else(|| "integer overflow when counting partitions".to_string())?,
                    sectors
                        .checked_add(sector_count)
                        .ok_or_else(|| "integer overflow when counting sectors".to_string())?,
                ))
            },
        )
    }

    /// Records the given sector bitfield at the given deadline/partition index.
    pub fn add(
        &mut self,
        deadline_idx: u64,
        partition_idx: u64,
        sector_numbers: BitField,
    ) -> Result<(), String> {
        if deadline_idx >= WPOST_PERIOD_DEADLINES {
            return Err(format!("invalid deadline {}", deadline_idx));
        }

        self.0
            .entry(deadline_idx)
            .or_default()
            .add(partition_idx, sector_numbers);

        Ok(())
    }

    /// Records the given sectors at the given deadline/partition index.
    pub fn add_values(
        &mut self,
        deadline_idx: u64,
        partition_idx: u64,
        sector_numbers: &[u64],
    ) -> Result<(), String> {
        self.add(
            deadline_idx,
            partition_idx,
            sector_numbers.iter().map(|&i| i as usize).collect(),
        )
    }

    /// Returns a sorted vec of deadlines in the map.
    pub fn deadlines(&self) -> Vec<u64> {
        let mut deadlines: Vec<_> = self.0.keys().copied().collect();
        deadlines.sort_unstable();
        deadlines
    }

    /// Walks the deadlines in deadline order.
    pub fn iter(&self) -> impl Iterator<Item = (u64, &PartitionSectorMap)> + '_ {
        self.deadlines().into_iter().map(move |i| (i, &self.0[&i]))
    }
}

/// Maps partitions to sector bitfields.
#[derive(Default)]
pub struct PartitionSectorMap(HashMap<u64, BitField>);

impl PartitionSectorMap {
    /// Records the given sectors at the given partition.
    pub fn add_values(&mut self, partition_idx: u64, sector_numbers: Vec<u64>) {
        self.add(
            partition_idx,
            sector_numbers.into_iter().map(|i| i as usize).collect(),
        );
    }
    /// Records the given sector bitfield at the given partition index, merging
    /// it with any existing bitfields if necessary.
    pub fn add(&mut self, partition_idx: u64, sector_numbers: BitField) {
        self.0
            .entry(partition_idx)
            .and_modify(|old_sector_numbers| *old_sector_numbers |= &sector_numbers)
            .or_insert(sector_numbers);
    }

    /// Counts the number of partitions & sectors within the map.
    pub fn count(&self) -> Result<(/* partitions */ u64, /* sectors */ u64), String> {
        let sectors = self
            .0
            .values()
            .map(|bf| bf.len())
            .try_fold(0_u64, |sectors, count| {
                sectors
                    .checked_add(count as u64)
                    .ok_or_else(|| "integer overflow when counting sectors".to_string())
            })?;
        Ok((self.0.len() as u64, sectors))
    }

    /// Returns a sorted vec of partitions in the map.
    pub fn partitions(&self) -> Vec<u64> {
        let mut partitions: Vec<_> = self.0.keys().copied().collect();
        partitions.sort_unstable();
        partitions
    }

    /// Walks the partitions in the map, in order of increasing index.
    pub fn iter(&self) -> impl Iterator<Item = (u64, &BitField)> + '_ {
        self.partitions().into_iter().map(move |i| (i, &self.0[&i]))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
