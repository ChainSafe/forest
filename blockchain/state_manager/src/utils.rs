// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::errors::*;
use crate::StateManager;
use cid::Cid;
use forest_actor_interface::{
    miner::{self, MinerInfo, Partition, SectorOnChainInfo, SectorPreCommitOnChainInfo},
    power,
};
use forest_blocks::Tipset;
use forest_fil_types::{verifier::ProofVerifier, RegisteredSealProof, SectorInfo, SectorNumber};
use forest_ipld_blockstore::BlockStore;
use fvm_ipld_bitfield::BitField;
use fvm_shared::address::Address;
use fvm_shared::randomness::Randomness;
use fvm_shared::version::NetworkVersion;
use serde::Serialize;

impl<DB> StateManager<DB>
where
    DB: BlockStore + Send + Sync + 'static,
{
    /// Retrieves and generates a vector of sector info for the winning `PoSt` verification.
    pub fn get_sectors_for_winning_post<V>(
        &self,
        st: &Cid,
        nv: NetworkVersion,
        miner_address: &Address,
        rand: Randomness,
    ) -> Result<Vec<SectorInfo>, anyhow::Error>
    where
        V: ProofVerifier,
    {
        let store = self.blockstore();

        let actor = self
            .get_actor(miner_address, *st)?
            .ok_or_else(|| Error::State("Miner actor address could not be resolved".to_string()))?;
        let mas = miner::State::load(self.blockstore(), &actor)?;

        let proving_sectors = {
            let mut proving_sectors = BitField::new();

            if nv < NetworkVersion::V7 {
                mas.for_each_deadline(store, |_, deadline| {
                    let mut fault_sectors = BitField::new();
                    deadline.for_each(store, |_, partition: miner::Partition| {
                        proving_sectors |= partition.all_sectors();
                        fault_sectors |= partition.faulty_sectors();
                        Ok(())
                    })?;

                    proving_sectors -= &fault_sectors;
                    Ok(())
                })?;
            } else {
                mas.for_each_deadline(store, |_, deadline| {
                    deadline.for_each(store, |_, partition: miner::Partition| {
                        proving_sectors |= &partition.active_sectors();
                        Ok(())
                    })?;
                    Ok(())
                })?;
            }
            proving_sectors
        };

        let num_prov_sect = proving_sectors.len() as u64;

        if num_prov_sect == 0 {
            return Ok(Vec::new());
        }

        let info = mas.info(store)?;

        let spt = RegisteredSealProof::from_sector_size(info.sector_size(), nv);

        let wpt = spt
            .registered_winning_post_proof()
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let m_id = miner_address.id()?;

        let ids = V::generate_winning_post_sector_challenge(wpt, m_id, rand, num_prov_sect)?;

        let mut iter = proving_sectors.iter();

        let mut selected_sectors = BitField::new();
        for n in ids {
            let sno = iter.nth(n as usize).ok_or_else(|| {
                anyhow::anyhow!(
                    "Error iterating over proving sectors, id {} does not exist",
                    n
                )
            })?;
            selected_sectors.set(sno);
        }

        let sectors = mas.load_sectors(store, Some(&selected_sectors))?;

        let out = sectors
            .into_iter()
            .map(|s_info| SectorInfo {
                proof: spt,
                sector_number: s_info.sector_number,
                sealed_cid: s_info.sealed_cid,
            })
            .collect();

        Ok(out)
    }

    /// Loads sectors for miner at given [Address].
    pub fn get_miner_sector_set<V>(
        &self,
        tipset: &Tipset,
        address: &Address,
        filter: Option<&BitField>,
    ) -> anyhow::Result<Vec<SectorOnChainInfo>, Error>
    where
        V: ProofVerifier,
    {
        let actor = self
            .get_actor(address, *tipset.parent_state())?
            .ok_or_else(|| Error::State("Miner actor address could not be resolved".to_string()))?;
        let mas = miner::State::load(self.blockstore(), &actor)?;

        Ok(mas.load_sectors(self.blockstore(), filter)?)
    }

    /// Returns miner's sector info for a given index.
    pub fn miner_sector_info<V>(
        &self,
        address: &Address,
        sector_number: SectorNumber,
        tipset: &Tipset,
    ) -> anyhow::Result<Option<SectorOnChainInfo>, Error>
    where
        V: ProofVerifier,
    {
        let actor = self
            .get_actor(address, *tipset.parent_state())?
            .ok_or_else(|| Error::State("Miner actor address could not be resolved".to_string()))?;
        let mas = miner::State::load(self.blockstore(), &actor)?;
        Ok(mas.get_sector(self.blockstore(), sector_number)?)
    }

    /// Returns the pre-committed sector info for a miner's sector.
    pub fn precommit_info<V>(
        &self,
        address: &Address,
        sector_number: &SectorNumber,
        tipset: &Tipset,
    ) -> anyhow::Result<SectorPreCommitOnChainInfo, Error>
    where
        V: ProofVerifier,
    {
        let actor = self
            .get_actor(address, *tipset.parent_state())?
            .ok_or_else(|| Error::State("Miner actor address could not be resolved".to_string()))?;
        let mas = miner::State::load(self.blockstore(), &actor)?;
        let precommit_info = mas.get_precommitted_sector(self.blockstore(), *sector_number)?;
        precommit_info.ok_or_else(|| Error::Other("precommit not found".to_string()))
    }

    /// Returns miner info at the given [`Tipset`]'s state.
    pub fn get_miner_info<V>(&self, tipset: &Tipset, address: &Address) -> anyhow::Result<MinerInfo>
    where
        V: ProofVerifier,
    {
        let actor = self
            .get_actor(address, *tipset.parent_state())?
            .ok_or_else(|| Error::State("Miner actor address could not be resolved".to_string()))?;
        let mas = miner::State::load(self.blockstore(), &actor)?;
        let info = mas.info(self.blockstore())?;
        Ok(info)
    }

    fn for_each_deadline_partition<V, F>(
        &self,
        tipset: &Tipset,
        address: &Address,
        mut cb: F,
    ) -> Result<(), anyhow::Error>
    where
        F: FnMut(&Partition) -> Result<(), anyhow::Error>,

        V: ProofVerifier,
    {
        let store = self.blockstore();

        let actor = self
            .get_actor(address, *tipset.parent_state())?
            .ok_or_else(|| Error::State("Miner actor address could not be resolved".to_string()))?;
        let mas = miner::State::load(self.blockstore(), &actor)?;

        mas.for_each_deadline(store, |_, deadline| {
            deadline.for_each(store, |_, partition: miner::Partition| {
                cb(&partition)?;
                Ok(())
            })?;
            Ok(())
        })?;

        Ok(())
    }

    /// Returns a `BitField` of all miner's faulty sectors.
    pub fn get_miner_faults<V>(
        &self,
        tipset: &Tipset,
        address: &Address,
    ) -> Result<BitField, anyhow::Error>
    where
        V: ProofVerifier,
    {
        let mut out = BitField::new();

        self.for_each_deadline_partition::<V, _>(tipset, address, |part| {
            out |= part.faulty_sectors();
            Ok(())
        })?;

        Ok(out)
    }

    /// Returns `BitField` of miner's recovering sectors.
    pub fn get_miner_recoveries<V>(
        &self,
        tipset: &Tipset,
        address: &Address,
    ) -> Result<BitField, anyhow::Error>
    where
        V: ProofVerifier,
    {
        let mut out = BitField::new();

        self.for_each_deadline_partition::<V, _>(tipset, address, |part| {
            out |= part.recovering_sectors();
            Ok(())
        })?;

        Ok(out)
    }

    /// Lists all miners that exist in the power actor state at given [`Tipset`].
    pub fn list_miner_actors(&self, tipset: &Tipset) -> anyhow::Result<Vec<Address>, Error> {
        let actor = self
            .get_actor(
                &forest_actor_interface::power::ADDRESS,
                *tipset.parent_state(),
            )?
            .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;
        let power_actor_state = power::State::load(self.blockstore(), &actor)?;

        let miners = power_actor_state.list_all_miners(self.blockstore())?;

        Ok(miners)
    }
}

/// JSON serialization formatted Deadline information.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Deadline {
    pub post_submissions: BitField,
}
