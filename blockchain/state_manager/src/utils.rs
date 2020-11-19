// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::errors::*;
use crate::StateManager;
use actor::miner::{self, Partition};
use actor::{
    miner::{ChainSectorInfo, Deadlines, MinerInfo, SectorOnChainInfo, SectorPreCommitOnChainInfo},
    power,
};
use address::Address;
use bitfield::BitField;
use blockstore::BlockStore;
use cid::Cid;
use encoding::serde_bytes::ByteBuf;
use fil_types::{
    verifier::ProofVerifier, Randomness, RegisteredSealProof, SectorInfo, SectorNumber,
    HAMT_BIT_WIDTH,
};
use forest_blocks::Tipset;
use ipld_amt::Amt;
use ipld_hamt::Hamt;
use std::convert::TryInto;
use std::error::Error as StdError;

impl<DB> StateManager<DB>
where
    DB: BlockStore + Send + Sync + 'static,
{
    /// Retrieves and generates a vector of sector info for the winning PoSt verification.
    pub fn get_sectors_for_winning_post<V>(
        &self,
        st: &Cid,
        miner_address: &Address,
        rand: Randomness,
    ) -> Result<Vec<SectorInfo>, Box<dyn StdError>>
    where
        V: ProofVerifier,
    {
        let store = self.blockstore();

        let mas: miner::State = self
            .load_actor_state(&miner_address, &st)
            .map_err(|err| format!("(get sectors) failed to load miner actor state: %{:}", err))?;

        let deadlines = mas.load_deadlines(store)?;

        let mut proving_sectors = BitField::new();

        deadlines.for_each(store, |_, deadline| {
            let partitions = deadline.partitions_amt(store)?;

            let mut fault_sectors = BitField::new();
            partitions.for_each(|_, partition: &miner::Partition| {
                proving_sectors |= &partition.sectors;
                fault_sectors |= &partition.faults;
                Ok(())
            })?;

            proving_sectors -= &fault_sectors;
            Ok(())
        })?;

        let num_prov_sect = proving_sectors.len() as u64;

        if num_prov_sect == 0 {
            return Ok(Vec::new());
        }

        let info = mas.get_info(store)?;

        let spt = RegisteredSealProof::from(info.sector_size);

        let wpt = spt.registered_winning_post_proof()?;

        let m_id = miner_address.id()?;

        let ids = V::generate_winning_post_sector_challenge(wpt, m_id, rand, num_prov_sect)?;

        let mut iter = proving_sectors.iter();

        let mut selected_sectors = BitField::new();
        for n in ids {
            let sno = iter.nth(n as usize).ok_or_else(|| {
                format!(
                    "Error iterating over proving sectors, id {} does not exist",
                    n
                )
            })?;
            selected_sectors.set(sno);
        }

        let sectors = mas.load_sector_infos(store, &selected_sectors)?;

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

    pub fn get_miner_sector_set<V>(
        &self,
        tipset: &Tipset,
        address: &Address,
        filter: &mut Option<&mut BitField>,
        filter_out: bool,
    ) -> Result<Vec<ChainSectorInfo>, Error>
    where
        V: ProofVerifier,
    {
        let miner_actor_state: miner::State = self
            .load_actor_state(&address, &tipset.parent_state())
            .map_err(|err| {
                Error::State(format!(
                    "(get miner sector set) failed to load miner actor state: {:}",
                    err
                ))
            })?;
        Self::load_sectors_from_set::<V>(
            self.blockstore(),
            &miner_actor_state.sectors,
            filter,
            filter_out,
        )
    }

    fn load_sectors_from_set<V>(
        block_store: &DB,
        ssc: &Cid,
        filter: &mut Option<&mut BitField>,
        _filter_out: bool,
    ) -> Result<Vec<ChainSectorInfo>, Error>
    where
        V: ProofVerifier,
    {
        let amt = Amt::load(ssc, block_store).map_err(|err| Error::Other(err.to_string()))?;

        let mut sset: Vec<ChainSectorInfo> = Vec::new();
        let for_each = |i: u64, sector_chain: &miner::SectorOnChainInfo| {
            if let Some(ref mut s) = filter {
                let i = i
                    .try_into()
                    .map_err(|_| "Could not convert from index to usize")?;
                if s.get(i) {
                    return Ok(());
                }
            }
            sset.push(ChainSectorInfo {
                info: sector_chain.clone(),
                id: i,
            });
            Ok(())
        };
        amt.for_each(for_each)
            .map_err(|err| Error::Other(format!("Error Processing ForEach {:}", err)))?;

        Ok(sset)
    }

    pub fn miner_sector_info<V>(
        &self,
        address: &Address,
        sector_number: &SectorNumber,
        tipset: &Tipset,
    ) -> Result<Option<SectorOnChainInfo>, Error>
    where
        V: ProofVerifier,
    {
        let miner_actor_state: miner::State = self
            .load_actor_state(&address, &tipset.parent_state())
            .map_err(|err| {
                Error::State(format!(
                    "(get miner sector info) failed to load miner actor state: {:}",
                    err
                ))
            })?;
        miner_actor_state
            .get_sector(self.blockstore(), *sector_number)
            .map_err(|err| Error::State(format!("(get sset) failed to get actor state: {:}", err)))
    }

    pub fn precommit_info<V>(
        &self,
        address: &Address,
        sector_number: &SectorNumber,
        tipset: &Tipset,
    ) -> Result<SectorPreCommitOnChainInfo, Error>
    where
        V: ProofVerifier,
    {
        let miner_actor_state: miner::State = self
            .load_actor_state(&address, &tipset.parent_state())
            .map_err(|err| {
                Error::State(format!(
                    "(get precommit info) failed to load miner actor state: {:}",
                    err
                ))
            })?;
        let precommit_info = miner_actor_state
            .get_precommitted_sector(self.blockstore(), *sector_number)
            .map_err(|err| {
                Error::Other(format!(
                    "(precommit info) failed to load miner actor state: %{:}",
                    err
                ))
            })?;
        Ok(precommit_info.ok_or_else(|| Error::Other("precommit not found".to_string()))?)
    }

    pub fn get_miner_info<V>(
        &self,
        tipset: &Tipset,
        address: &Address,
    ) -> Result<MinerInfo, Box<dyn StdError>>
    where
        V: ProofVerifier,
    {
        let miner_actor_state: miner::State = self
            .load_actor_state(&address, &tipset.parent_state())
            .map_err(|err| {
                Error::State(format!(
                    "(get miner info) failed to load miner actor state: {:}",
                    err
                ))
            })?;
        Ok(miner_actor_state.get_info(self.blockstore())?)
    }

    pub fn get_miner_deadlines<V>(
        &self,
        tipset: &Tipset,
        address: &Address,
    ) -> Result<Deadlines, Error>
    where
        V: ProofVerifier,
    {
        let miner_actor_state: miner::State = self
            .load_actor_state(&address, &tipset.parent_state())
            .map_err(|err| {
                Error::State(format!(
                    "(get miner deadlines) failed to load miner actor state: {:}",
                    err
                ))
            })?;
        miner_actor_state
            .load_deadlines(&*self.blockstore_cloned())
            .map_err(|err| {
                Error::State(format!(
                    "(get_miner_deadlines) could not load deadlines: {:}",
                    err
                ))
            })
    }

    fn for_each_deadline_partition<V, F>(
        &self,
        tipset: &Tipset,
        address: &Address,
        mut cb: F,
    ) -> Result<(), Box<dyn StdError>>
    where
        F: FnMut(&Partition) -> Result<(), Box<dyn StdError>>,

        V: ProofVerifier,
    {
        let store = self.blockstore();

        // TODO clean this logic up
        let miner_actor_state: miner::State =
            self.load_actor_state(&address, tipset.parent_state())?;
        let deadlines = miner_actor_state.load_deadlines(store)?;
        deadlines.for_each(store, |_, deadline| {
            let partitions = deadline.partitions_amt(store).map_err(|e| e.to_string())?;
            partitions
                .for_each(|_, part| {
                    cb(part)?;
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
            Ok(())
        })?;

        Ok(())
    }

    pub fn get_miner_faults<V>(
        &self,
        tipset: &Tipset,
        address: &Address,
    ) -> Result<BitField, Box<dyn StdError>>
    where
        V: ProofVerifier,
    {
        let mut out = BitField::new();

        self.for_each_deadline_partition::<V, _>(tipset, address, |part| {
            out |= &part.faults;
            Ok(())
        })?;

        Ok(out)
    }

    pub fn get_miner_recoveries<V>(
        &self,
        tipset: &Tipset,
        address: &Address,
    ) -> Result<BitField, Box<dyn StdError>>
    where
        V: ProofVerifier,
    {
        let mut out = BitField::new();

        self.for_each_deadline_partition::<V, _>(tipset, address, |part| {
            out |= &part.recoveries;
            Ok(())
        })?;

        Ok(out)
    }

    pub fn list_miner_actors<V>(&self, tipset: &Tipset) -> Result<Vec<Address>, Error>
    where
        V: ProofVerifier,
    {
        let power_actor_state: power::State = self
            .load_actor_state(&actor::STORAGE_POWER_ACTOR_ADDR, &tipset.parent_state())
            .map_err(|err| {
                Error::State(format!(
                    "(get sset) failed to load power actor state: {:}",
                    err
                ))
            })?;
        let mut miners: Vec<Address> = Vec::new();
        let block_store = &*self.blockstore_cloned();
        let map = Hamt::<_, _>::load_with_bit_width(
            &power_actor_state.claims,
            block_store,
            HAMT_BIT_WIDTH,
        )
        .map_err(|err| Error::Other(err.to_string()))?;
        map.for_each(|_, k: &ByteBuf| {
            let address = Address::from_bytes(k.as_ref())?;
            miners.push(address);
            Ok(())
        })
        .map_err(|e| Error::Other(e.to_string()))?;
        Ok(miners)
    }
}
