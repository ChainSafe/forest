// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use forest_actor_interface::miner;
use forest_db::Store;
use forest_fil_types::verifier::generate_winning_post_sector_challenge;
use forest_shim::{
    randomness::Randomness,
    sector::{RegisteredSealProof, SectorInfo},
    version::NetworkVersion,
};
use fvm_ipld_bitfield::BitField;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::address::Address;

use crate::{errors::*, StateManager};

impl<DB> StateManager<DB>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
{
    /// Retrieves and generates a vector of sector info for the winning `PoSt`
    /// verification.
    pub fn get_sectors_for_winning_post(
        &self,
        st: &Cid,
        nv: NetworkVersion,
        miner_address: &Address,
        rand: Randomness,
    ) -> Result<Vec<SectorInfo>, anyhow::Error> {
        let store = self.blockstore();

        let actor = self
            .get_actor(miner_address, *st)?
            .ok_or_else(|| Error::State("Miner actor address could not be resolved".to_string()))?;
        let mas = miner::State::load(self.blockstore(), &actor)?;

        let proving_sectors = {
            let mut proving_sectors = BitField::new();

            if nv < NetworkVersion::V7 {
                mas.for_each_deadline(&self.chain_config.policy, store, |_, deadline| {
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
                mas.for_each_deadline(&self.chain_config.policy, store, |_, deadline| {
                    deadline.for_each(store, |_, partition: miner::Partition| {
                        proving_sectors |= &partition.active_sectors();
                        Ok(())
                    })?;
                    Ok(())
                })?;
            }
            proving_sectors
        };

        let num_prov_sect = proving_sectors.len();

        if num_prov_sect == 0 {
            return Ok(Vec::new());
        }

        let info = mas.info(store)?;
        let spt = RegisteredSealProof::from_sector_size(info.sector_size(), nv);

        let wpt = spt
            .registered_winning_post_proof()
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let m_id = miner_address.id()?;

        let ids =
            generate_winning_post_sector_challenge(wpt.into(), m_id, rand.into(), num_prov_sect)?;

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
            .map(|s_info| SectorInfo::new(*spt, s_info.sector_number, s_info.sealed_cid))
            .collect();

        Ok(out)
    }
}
