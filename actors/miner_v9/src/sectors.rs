// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::BTreeSet;

use anyhow::anyhow;
use cid::Cid;
use fil_actors_runtime_v9::{actor_error, ActorDowncast, ActorError, Array};
use fvm_ipld_amt::Error as AmtError;
use fvm_ipld_bitfield::BitField;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::error::ExitCode;
use fvm_shared::sector::{SectorNumber, MAX_SECTOR_NUMBER};

use super::SectorOnChainInfo;

pub struct Sectors<'db, BS> {
    pub amt: Array<'db, SectorOnChainInfo, BS>,
}

impl<'db, BS: Blockstore> Sectors<'db, BS> {
    pub fn load(store: &'db BS, root: &Cid) -> Result<Self, AmtError> {
        Ok(Self {
            amt: Array::load(root, store)?,
        })
    }

    pub fn load_sector(
        &self,
        sector_numbers: &BitField,
    ) -> Result<Vec<SectorOnChainInfo>, ActorError> {
        let mut sector_infos: Vec<SectorOnChainInfo> = Vec::new();
        for sector_number in sector_numbers.iter() {
            let sector_on_chain = self
                .amt
                .get(sector_number)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::USR_ILLEGAL_STATE,
                        format!("failed to load sector {}", sector_number),
                    )
                })?
                .cloned()
                .ok_or_else(|| actor_error!(not_found; "sector not found: {}", sector_number))?;
            sector_infos.push(sector_on_chain);
        }
        Ok(sector_infos)
    }

    pub fn get(&self, sector_number: SectorNumber) -> anyhow::Result<Option<SectorOnChainInfo>> {
        Ok(self
            .amt
            .get(sector_number)
            .map_err(|e| e.downcast_wrap(format!("failed to get sector {}", sector_number)))?
            .cloned())
    }

    pub fn store(&mut self, infos: Vec<SectorOnChainInfo>) -> anyhow::Result<()> {
        for info in infos {
            let sector_number = info.sector_number;

            if sector_number > MAX_SECTOR_NUMBER {
                return Err(anyhow!("sector number {} out of range", info.sector_number));
            }

            self.amt.set(sector_number, info).map_err(|e| {
                e.downcast_wrap(format!("failed to store sector {}", sector_number))
            })?;
        }

        Ok(())
    }

    pub fn must_get(&self, sector_number: SectorNumber) -> anyhow::Result<SectorOnChainInfo> {
        self.get(sector_number)?
            .ok_or_else(|| anyhow!("sector {} not found", sector_number))
    }

    /// Loads info for a set of sectors to be proven.
    /// If any of the sectors are declared faulty and not to be recovered, info for the first non-faulty sector is substituted instead.
    /// If any of the sectors are declared recovered, they are returned from this method.
    pub fn load_for_proof(
        &self,
        proven_sectors: &BitField,
        expected_faults: &BitField,
    ) -> anyhow::Result<Vec<SectorOnChainInfo>> {
        let non_faults = proven_sectors - expected_faults;

        if non_faults.is_empty() {
            return Ok(Vec::new());
        }

        let good_sector_number = non_faults.first().expect("faults are not empty");

        let sector_infos = self.load_with_fault_max(
            proven_sectors,
            expected_faults,
            good_sector_number as SectorNumber,
        )?;

        Ok(sector_infos)
    }
    /// Loads sector info for a sequence of sectors, substituting info for a stand-in sector for any that are faulty.
    pub fn load_with_fault_max(
        &self,
        sectors: &BitField,
        faults: &BitField,
        fault_stand_in: SectorNumber,
    ) -> anyhow::Result<Vec<SectorOnChainInfo>> {
        let stand_in_info = self.must_get(fault_stand_in)?;

        // Expand faults into a map for quick lookups.
        // The faults bitfield should already be a subset of the sectors bitfield.
        let sector_count = sectors.len();

        let fault_set: BTreeSet<u64> = faults.iter().collect();

        let mut sector_infos = Vec::with_capacity(sector_count as usize);
        for i in sectors.iter() {
            let faulty = fault_set.contains(&i);
            let sector = if !faulty {
                self.must_get(i)?
            } else {
                stand_in_info.clone()
            };
            sector_infos.push(sector);
        }

        Ok(sector_infos)
    }
}

pub fn select_sectors(
    sectors: &[SectorOnChainInfo],
    field: &BitField,
) -> anyhow::Result<Vec<SectorOnChainInfo>> {
    let mut to_include: BTreeSet<_> = field.iter().collect();
    let included = sectors
        .iter()
        .filter(|si| to_include.remove(&si.sector_number))
        .cloned()
        .collect();

    if !to_include.is_empty() {
        return Err(anyhow!(
            "failed to find {} expected sectors",
            to_include.len()
        ));
    }

    Ok(included)
}
