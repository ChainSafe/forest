// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::BTreeMap;

use anyhow::anyhow;
use fvm_ipld_bitfield::{BitField, Validate};
use serde::{Deserialize, Serialize};

use fil_actors_runtime_v9::runtime::Policy;

/// Maps deadlines to partition maps.
#[derive(Default)]
pub struct DeadlineSectorMap(BTreeMap<u64, PartitionSectorMap>);

impl DeadlineSectorMap {
    pub fn new() -> Self {
        Default::default()
    }

    /// Check validates all bitfields and counts the number of partitions & sectors
    /// contained within the map, and returns an error if they exceed the given
    /// maximums.
    pub fn check(&mut self, max_partitions: u64, max_sectors: u64) -> anyhow::Result<()> {
        let (partition_count, sector_count) = self
            .count()
            .map_err(|e| anyhow!("failed to count sectors: {:?}", e))?;

        if partition_count > max_partitions {
            return Err(anyhow!(
                "too many partitions {}, max {}",
                partition_count,
                max_partitions
            ));
        }

        if sector_count > max_sectors {
            return Err(anyhow!(
                "too many sectors {}, max {}",
                sector_count,
                max_sectors
            ));
        }

        Ok(())
    }

    /// Counts the number of partitions & sectors within the map.
    pub fn count(&mut self) -> anyhow::Result<(/* partitions */ u64, /* sectors */ u64)> {
        self.0.iter_mut().try_fold(
            (0_u64, 0_u64),
            |(partitions, sectors), (deadline_idx, pm)| {
                let (partition_count, sector_count) = pm
                    .count()
                    .map_err(|e| anyhow!("when counting deadline {}: {:?}", deadline_idx, e))?;
                Ok((
                    partitions
                        .checked_add(partition_count)
                        .ok_or_else(|| anyhow!("integer overflow when counting partitions"))?,
                    sectors
                        .checked_add(sector_count)
                        .ok_or_else(|| anyhow!("integer overflow when counting sectors"))?,
                ))
            },
        )
    }

    /// Records the given sector bitfield at the given deadline/partition index.
    pub fn add(
        &mut self,
        policy: &Policy,
        deadline_idx: u64,
        partition_idx: u64,
        sector_numbers: BitField,
    ) -> anyhow::Result<()> {
        if deadline_idx >= policy.wpost_period_deadlines {
            return Err(anyhow!("invalid deadline {}", deadline_idx));
        }

        self.0
            .entry(deadline_idx)
            .or_default()
            .add(partition_idx, sector_numbers)
    }

    /// Records the given sectors at the given deadline/partition index.
    pub fn add_values(
        &mut self,
        policy: &Policy,
        deadline_idx: u64,
        partition_idx: u64,
        sector_numbers: &[u64],
    ) -> anyhow::Result<()> {
        self.add(
            policy,
            deadline_idx,
            partition_idx,
            BitField::try_from_bits(sector_numbers.iter().copied())?,
        )
    }

    /// Returns a sorted vec of deadlines in the map.
    pub fn deadlines(&self) -> impl Iterator<Item = u64> + '_ {
        self.0.keys().copied()
    }

    /// Walks the deadlines in deadline order.
    pub fn iter(&mut self) -> impl Iterator<Item = (u64, &mut PartitionSectorMap)> + '_ {
        self.0.iter_mut().map(|(&i, x)| (i, x))
    }
}

/// Maps partitions to sector bitfields.
#[derive(Default, Serialize, Deserialize)]
pub struct PartitionSectorMap(BTreeMap<u64, BitField>);

impl PartitionSectorMap {
    /// Records the given sectors at the given partition.
    pub fn add_values(
        &mut self,
        partition_idx: u64,
        sector_numbers: Vec<u64>,
    ) -> anyhow::Result<()> {
        self.add(partition_idx, BitField::try_from_bits(sector_numbers)?)
    }
    /// Records the given sector bitfield at the given partition index, merging
    /// it with any existing bitfields if necessary.
    pub fn add(&mut self, partition_idx: u64, sector_numbers: BitField) -> anyhow::Result<()> {
        match self.0.get_mut(&partition_idx) {
            Some(old_sector_numbers) => {
                *old_sector_numbers |= &sector_numbers;
            }
            None => {
                self.0.insert(partition_idx, sector_numbers);
            }
        }
        Ok(())
    }

    /// Counts the number of partitions & sectors within the map.
    pub fn count(&mut self) -> anyhow::Result<(/* partitions */ u64, /* sectors */ u64)> {
        let sectors = self
            .0
            .iter_mut()
            .try_fold(0_u64, |sectors, (partition_idx, bf)| {
                let validated = bf.validate().map_err(|e| {
                    anyhow!(
                        "failed to parse bitmap for partition {}: {}",
                        partition_idx,
                        e
                    )
                })?;
                sectors
                    .checked_add(validated.len())
                    .ok_or_else(|| anyhow!("integer overflow when counting sectors"))
            })?;
        Ok((self.0.len() as u64, sectors))
    }

    /// Returns a sorted vec of partitions in the map.
    pub fn partitions(&self) -> impl Iterator<Item = u64> + '_ {
        self.0.keys().copied()
    }

    /// Walks the partitions in the map, in order of increasing index.
    pub fn iter(&mut self) -> impl Iterator<Item = (u64, &mut BitField)> + '_ {
        self.0.iter_mut().map(|(&i, x)| (i, x))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
