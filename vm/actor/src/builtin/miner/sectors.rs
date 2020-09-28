// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::SectorOnChainInfo;
use crate::{actor_error, ActorError, ExitCode};
use bitfield::BitField;
use cid::Cid;
use fil_types::{SectorNumber, MAX_SECTOR_NUMBER};
use ipld_amt::{Amt, Error as AmtError};
use ipld_blockstore::BlockStore;

pub struct Sectors<'db, BS> {
    pub amt: Amt<'db, SectorOnChainInfo, BS>,
}

impl<'db, BS: BlockStore> Sectors<'db, BS> {
    pub fn load(store: &'db BS, root: &Cid) -> Result<Self, AmtError> {
        Ok(Self {
            amt: Amt::load(root, store)?,
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
                .get(sector_number as SectorNumber)
                .map_err(|e| {
                    actor_error!(
                        ErrIllegalState,
                        "failed to load sector {}: {:?}",
                        sector_number,
                        e
                    )
                })?
                .ok_or_else(|| actor_error!(ErrNotFound; "sector not found: {}", sector_number))?;
            sector_infos.push(sector_on_chain);
        }
        Ok(sector_infos)
    }

    pub fn get(&self, sector_number: SectorNumber) -> Result<Option<SectorOnChainInfo>, String> {
        self.amt
            .get(sector_number)
            .map_err(|e| format!("failed to get sector {}: {:?}", sector_number, e))
    }

    pub fn store(&mut self, infos: Vec<SectorOnChainInfo>) -> Result<(), String> {
        for info in infos {
            let sector_number = info.sector_number;

            #[allow(clippy::absurd_extreme_comparisons)]
            if sector_number > MAX_SECTOR_NUMBER {
                return Err(format!("sector number {} out of range", info.sector_number));
            }

            self.amt
                .set(sector_number, info)
                .map_err(|e| format!("failed to store sector {}: {:?}", sector_number, e))?;
        }

        if self.amt.count() > super::SECTORS_MAX as u64 {
            return Err("too many sectors".to_string());
        }

        Ok(())
    }

    pub fn must_get(&self, sector_number: SectorNumber) -> Result<SectorOnChainInfo, String> {
        self.get(sector_number)?
            .ok_or_else(|| format!("sector {} not found", sector_number))
    }
}
