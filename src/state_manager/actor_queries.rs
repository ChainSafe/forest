// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::miner::{MinerInfo, MinerPower, Partition};
use crate::shim::actors::verifreg::ext::VerifiedRegistryStateExt as _;
use crate::shim::actors::verifreg::{Allocation, AllocationID, Claim};
use ahash::HashMap;
use fil_actor_verifreg_state::v12::DataCap;
use fil_actor_verifreg_state::v13::ClaimID;
use fil_actors_shared::fvm_ipld_bitfield::BitField;

impl<DB> StateManager<DB>
where
    DB: Blockstore + Send + Sync + 'static,
{
    /// Retrieves market state
    pub fn market_state(&self, ts: &Tipset) -> Result<market::State, Error> {
        let actor = self.get_required_actor(&Address::MARKET_ACTOR, *ts.parent_state())?;
        let market_state = market::State::load(self.blockstore(), actor.code, actor.state)?;
        Ok(market_state)
    }

    /// Retrieves market balance in escrow and locked tables.
    pub fn market_balance(&self, addr: &Address, ts: &Tipset) -> Result<MarketBalance, Error> {
        let market_state = self.market_state(ts)?;
        let new_addr = self.lookup_required_id(addr, ts)?;
        let out = MarketBalance {
            escrow: {
                market_state
                    .escrow_table(self.blockstore())?
                    .get(&new_addr)?
            },
            locked: {
                market_state
                    .locked_table(self.blockstore())?
                    .get(&new_addr)?
            },
        };

        Ok(out)
    }

    /// Retrieves miner info.
    pub fn miner_info(&self, addr: &Address, ts: &Tipset) -> Result<MinerInfo, Error> {
        let actor = self.get_actor(addr, *ts.parent_state())?.ok_or_else(|| {
            Error::state(format!(
                "Miner actor {addr} not found at epoch {}",
                ts.epoch()
            ))
        })?;
        let state = miner::State::load(self.blockstore(), actor.code, actor.state)?;

        Ok(state.info(self.blockstore())?)
    }

    /// Retrieves miner faults.
    pub fn miner_faults(&self, addr: &Address, ts: &Tipset) -> Result<BitField, Error> {
        self.all_partition_sectors(addr, ts, |partition| partition.faulty_sectors().clone())
    }

    /// Retrieves miner recoveries.
    pub fn miner_recoveries(&self, addr: &Address, ts: &Tipset) -> Result<BitField, Error> {
        self.all_partition_sectors(addr, ts, |partition| partition.recovering_sectors().clone())
    }

    fn all_partition_sectors(
        &self,
        addr: &Address,
        ts: &Tipset,
        get_sector: impl Fn(Partition<'_>) -> BitField,
    ) -> Result<BitField, Error> {
        let actor = self.get_actor(addr, *ts.parent_state())?.ok_or_else(|| {
            Error::state(format!(
                "Miner actor {addr} not found at epoch {}",
                ts.epoch()
            ))
        })?;

        let state = miner::State::load(self.blockstore(), actor.code, actor.state)?;

        let mut partitions = Vec::new();

        state.for_each_deadline(
            &self.chain_config().policy,
            self.blockstore(),
            |_, deadline| {
                deadline.for_each(self.blockstore(), |_, partition| {
                    partitions.push(get_sector(partition));
                    Ok(())
                })
            },
        )?;

        Ok(BitField::union(partitions.iter()))
    }

    /// Retrieves miner power.
    pub fn miner_power(&self, addr: &Address, ts: &Tipset) -> Result<MinerPower, Error> {
        if let Some((miner_power, total_power)) = self.get_power(ts.parent_state(), Some(addr))? {
            return Ok(MinerPower {
                miner_power,
                total_power,
                has_min_power: true,
            });
        }

        Ok(MinerPower {
            has_min_power: false,
            miner_power: Default::default(),
            total_power: Default::default(),
        })
    }

    pub fn get_verified_registry_actor_state(
        &self,
        ts: &Tipset,
    ) -> anyhow::Result<verifreg::State> {
        let act = self
            .get_actor(&Address::VERIFIED_REGISTRY_ACTOR, *ts.parent_state())
            .map_err(Error::state)?
            .ok_or_else(|| Error::state("actor not found"))?;
        verifreg::State::load(self.blockstore(), act.code, act.state)
    }
    pub fn get_claim(
        &self,
        addr: &Address,
        ts: &Tipset,
        claim_id: ClaimID,
    ) -> anyhow::Result<Option<Claim>> {
        let id_address = self.lookup_required_id(addr, ts)?;
        let state = self.get_verified_registry_actor_state(ts)?;
        state.get_claim(self.blockstore(), id_address, claim_id)
    }

    pub fn get_all_claims(&self, ts: &Tipset) -> anyhow::Result<HashMap<ClaimID, Claim>> {
        let state = self.get_verified_registry_actor_state(ts)?;
        state.get_all_claims(self.blockstore())
    }

    pub fn get_allocation(
        &self,
        addr: &Address,
        ts: &Tipset,
        allocation_id: AllocationID,
    ) -> anyhow::Result<Option<Allocation>> {
        let id_address = self.lookup_required_id(addr, ts)?;
        let state = self.get_verified_registry_actor_state(ts)?;
        state.get_allocation(self.blockstore(), id_address.id()?, allocation_id)
    }

    pub fn get_all_allocations(
        &self,
        ts: &Tipset,
    ) -> anyhow::Result<HashMap<AllocationID, Allocation>> {
        let state = self.get_verified_registry_actor_state(ts)?;
        state.get_all_allocations(self.blockstore())
    }

    pub fn verified_client_status(
        &self,
        addr: &Address,
        ts: &Tipset,
    ) -> anyhow::Result<Option<DataCap>> {
        let id = self.lookup_required_id(addr, ts)?;
        let network_version = self.get_network_version(ts.epoch());

        // This is a copy of Lotus code, we need to treat all the actors below version 9
        // differently. Which maps to network below version 17.
        // Original: https://github.com/filecoin-project/lotus/blob/5e76b05b17771da6939c7b0bf65127c3dc70ee23/node/impl/full/state.go#L1627-L1664.
        if (u32::from(network_version.0)) < 17 {
            let state = self.get_verified_registry_actor_state(ts)?;
            return state.verified_client_data_cap(self.blockstore(), id);
        }

        let act = self
            .get_actor(&Address::DATACAP_TOKEN_ACTOR, *ts.parent_state())
            .map_err(Error::state)?
            .ok_or_else(|| {
                Error::state(format!(
                    "Data cap actor {} not found",
                    Address::DATACAP_TOKEN_ACTOR
                ))
            })?;

        let state = datacap::State::load(self.blockstore(), act.code, act.state)?;

        state.verified_client_data_cap(self.blockstore(), id)
    }
}
