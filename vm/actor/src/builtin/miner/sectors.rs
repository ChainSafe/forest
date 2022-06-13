// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::SectorOnChainInfo;
use crate::{actor_error, ActorDowncast, ActorError, ExitCode};
use ahash::AHashSet;
use bitfield::BitField;
use cid::Cid;
use fil_types::{SectorNumber, MAX_SECTOR_NUMBER};
use ipld_amt::{Amt, Error as AmtError};
use ipld_blockstore::BlockStore;
use std::collections::HashSet;
use std::error::Error as StdError;

pub struct Sectors<'db, BS> {
    pub amt: Amt<'db, SectorOnChainInfo, BS>,
}

impl<'db, BS: BlockStore> Sectors<'db, BS> {
    pub fn load(store: &'db BS, root: &Cid) -> Result<Self, AmtError> {
        Ok(Self {
            amt: Amt::load(root, store)?,
        })
    }

    pub fn load_sector<'a>(
        &self,
        sector_numbers: impl bitfield::Validate<'a>,
    ) -> Result<Vec<SectorOnChainInfo>, ActorError> {
        let sector_numbers = match sector_numbers.validate() {
            Ok(sector_numbers) => sector_numbers,
            Err(e) => {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "failed to load sectors: {}",
                    e
                ))
            }
        };

        let mut sector_infos: Vec<SectorOnChainInfo> = Vec::new();
        for sector_number in sector_numbers.iter() {
            let sector_on_chain = self
                .amt
                .get(sector_number)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to load sector {}", sector_number),
                    )
                })?
                .cloned()
                .ok_or_else(|| actor_error!(ErrNotFound; "sector not found: {}", sector_number))?;
            sector_infos.push(sector_on_chain);
        }
        Ok(sector_infos)
    }

    pub fn get(
        &self,
        sector_number: SectorNumber,
    ) -> Result<Option<SectorOnChainInfo>, Box<dyn StdError>> {
        Ok(self
            .amt
            .get(sector_number as usize)
            .map_err(|e| e.downcast_wrap(format!("failed to get sector {}", sector_number)))?
            .cloned())
    }

    pub fn store(&mut self, infos: Vec<SectorOnChainInfo>) -> Result<(), Box<dyn StdError>> {
        for info in infos {
            let sector_number = info.sector_number;

            if sector_number > MAX_SECTOR_NUMBER {
                return Err(format!("sector number {} out of range", info.sector_number).into());
            }

            self.amt.set(sector_number as usize, info).map_err(|e| {
                e.downcast_wrap(format!("failed to store sector {}", sector_number))
            })?;
        }

        Ok(())
    }

    pub fn must_get(
        &self,
        sector_number: SectorNumber,
    ) -> Result<SectorOnChainInfo, Box<dyn StdError>> {
        self.get(sector_number)?
            .ok_or_else(|| format!("sector {} not found", sector_number).into())
    }

    /// Loads info for a set of sectors to be proven.
    /// If any of the sectors are declared faulty and not to be recovered, info for the first non-faulty sector is substituted instead.
    /// If any of the sectors are declared recovered, they are returned from this method.
    pub fn load_for_proof(
        &self,
        proven_sectors: &BitField,
        expected_faults: &BitField,
    ) -> Result<Vec<SectorOnChainInfo>, Box<dyn StdError>> {
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
    ) -> Result<Vec<SectorOnChainInfo>, Box<dyn StdError>> {
        let stand_in_info = self.must_get(fault_stand_in)?;

        // Expand faults into a map for quick lookups.
        // The faults bitfield should already be a subset of the sectors bitfield.
        let sector_count = sectors.len();

        let fault_set: HashSet<usize> = faults.iter().collect();

        let mut sector_infos = Vec::with_capacity(sector_count);
        for i in sectors.iter() {
            let faulty = fault_set.contains(&i);
            let sector = if !faulty {
                self.must_get(i as u64)?
            } else {
                stand_in_info.clone()
            };
            sector_infos.push(sector);
        }

        Ok(sector_infos)
    }
}

pub(crate) fn select_sectors(
    sectors: &[SectorOnChainInfo],
    field: &BitField,
) -> Result<Vec<SectorOnChainInfo>, Box<dyn StdError>> {
    let mut to_include: AHashSet<_> = field.iter().collect();

    let mut included = Vec::with_capacity(to_include.len());
    for s in sectors {
        let sec = s.sector_number as usize;
        if !to_include.contains(&sec) {
            continue;
        }
        included.push(s.clone());
        to_include.remove(&sec);
    }

    if !to_include.is_empty() {
        return Err(format!("failed to find {} expected sectors", to_include.len()).into());
    }

    Ok(included)
}
