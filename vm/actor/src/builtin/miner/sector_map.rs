// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::WPOST_PERIOD_DEADLINES;
use bitfield::{BitField, UnvalidatedBitField, Validate};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Maps deadlines to partition maps.
#[derive(Default)]
pub struct DeadlineSectorMap(HashMap<usize, PartitionSectorMap>);

impl DeadlineSectorMap {
    pub fn new() -> Self {
        Default::default()
    }

    /// Check validates all bitfields and counts the number of partitions & sectors
    /// contained within the map, and returns an error if they exceed the given
    /// maximums.
    pub fn check(&mut self, max_partitions: u64, max_sectors: u64) -> Result<(), String> {
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
    pub fn count(&mut self) -> Result<(/* partitions */ u64, /* sectors */ u64), String> {
        self.0.iter_mut().try_fold(
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
        deadline_idx: usize,
        partition_idx: usize,
        sector_numbers: UnvalidatedBitField,
    ) -> Result<(), String> {
        if deadline_idx >= WPOST_PERIOD_DEADLINES as usize {
            return Err(format!("invalid deadline {}", deadline_idx));
        }

        self.0
            .entry(deadline_idx)
            .or_default()
            .add(partition_idx, sector_numbers)
    }

    /// Records the given sectors at the given deadline/partition index.
    pub fn add_values(
        &mut self,
        deadline_idx: usize,
        partition_idx: usize,
        sector_numbers: &[u64],
    ) -> Result<(), String> {
        self.add(
            deadline_idx,
            partition_idx,
            sector_numbers
                .iter()
                .map(|&i| i as usize)
                .collect::<BitField>()
                .into(),
        )
    }

    /// Returns a sorted vec of deadlines in the map.
    pub fn deadlines(&self) -> Vec<usize> {
        let mut deadlines: Vec<_> = self.0.keys().copied().collect();
        deadlines.sort_unstable();
        deadlines
    }

    /// Walks the deadlines in deadline order.
    pub fn iter(&mut self) -> impl Iterator<Item = (usize, &mut PartitionSectorMap)> + '_ {
        let mut vec: Vec<_> = self.0.iter_mut().map(|(&i, x)| (i, x)).collect();
        vec.sort_unstable_by_key(|&(i, _)| i);
        vec.into_iter()
    }
}

/// Maps partitions to sector bitfields.
#[derive(Default, Serialize, Deserialize)]
pub struct PartitionSectorMap(HashMap<usize, UnvalidatedBitField>);

impl PartitionSectorMap {
    /// Records the given sectors at the given partition.
    pub fn add_values(
        &mut self,
        partition_idx: usize,
        sector_numbers: Vec<u64>,
    ) -> Result<(), String> {
        self.add(
            partition_idx,
            sector_numbers
                .into_iter()
                .map(|i| i as usize)
                .collect::<BitField>()
                .into(),
        )
    }
    /// Records the given sector bitfield at the given partition index, merging
    /// it with any existing bitfields if necessary.
    pub fn add(
        &mut self,
        partition_idx: usize,
        mut sector_numbers: UnvalidatedBitField,
    ) -> Result<(), String> {
        match self.0.get_mut(&partition_idx) {
            Some(old_sector_numbers) => {
                let old = old_sector_numbers
                    .validate_mut()
                    .map_err(|e| format!("failed to validate sector bitfield: {}", e))?;
                let new = sector_numbers
                    .validate()
                    .map_err(|e| format!("failed to validate new sector bitfield: {}", e))?;
                *old |= new;
            }
            None => {
                self.0.insert(partition_idx, sector_numbers);
            }
        }
        Ok(())
    }

    /// Counts the number of partitions & sectors within the map.
    pub fn count(&mut self) -> Result<(/* partitions */ u64, /* sectors */ u64), String> {
        let sectors = self
            .0
            .iter_mut()
            .try_fold(0_u64, |sectors, (partition_idx, bf)| {
                let validated = bf.validate().map_err(|e| {
                    format!(
                        "failed to parse bitmap for partition {}: {}",
                        partition_idx, e
                    )
                })?;
                sectors
                    .checked_add(validated.len() as u64)
                    .ok_or_else(|| "integer overflow when counting sectors".to_string())
            })?;
        Ok((self.0.len() as u64, sectors))
    }

    /// Returns a sorted vec of partitions in the map.
    pub fn partitions(&self) -> Vec<usize> {
        let mut partitions: Vec<_> = self.0.keys().copied().collect();
        partitions.sort_unstable();
        partitions
    }

    /// Walks the partitions in the map, in order of increasing index.
    pub fn iter(&mut self) -> impl Iterator<Item = (usize, &mut UnvalidatedBitField)> + '_ {
        let mut vec: Vec<_> = self.0.iter_mut().map(|(&i, x)| (i, x)).collect();
        vec.sort_unstable_by_key(|&(i, _)| i);
        vec.into_iter()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
