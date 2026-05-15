// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::chain_rand::draw_randomness;
use super::*;
use crate::beacon::BeaconEntry;
use crate::rpc::types::MiningBaseInfo;
use crate::shim::randomness::Randomness;
use crate::shim::runtime::Policy;
use anyhow::Context as _;
use fil_actors_shared::v12::runtime::DomainSeparationTag;
use fvm_ipld_encoding::to_vec;
use num::BigInt;
use num_traits::identities::Zero;

impl StateManager {
    /// Checks the eligibility of the miner. This is used in the validation that
    /// a block's miner has the requirements to mine a block.
    pub fn eligible_to_mine(
        &self,
        address: &Address,
        base_tipset: &Tipset,
        lookback_tipset: &Tipset,
    ) -> anyhow::Result<bool, Error> {
        let hmp =
            self.miner_has_min_power(&self.chain_config().policy, address, lookback_tipset)?;
        let version = self.get_network_version(base_tipset.epoch());

        if version <= NetworkVersion::V3 {
            return Ok(hmp);
        }

        if !hmp {
            return Ok(false);
        }

        let actor = self
            .get_actor(&Address::POWER_ACTOR, *base_tipset.parent_state())?
            .ok_or_else(|| Error::state("Power actor address could not be resolved"))?;

        let power_state = power::State::load(self.db(), actor.code, actor.state)?;

        let actor = self
            .get_actor(address, *base_tipset.parent_state())?
            .ok_or_else(|| Error::state("Miner actor address could not be resolved"))?;

        let miner_state = miner::State::load(self.db(), actor.code, actor.state)?;

        // Non-empty power claim.
        let claim = power_state
            .miner_power(self.db(), address)?
            .ok_or_else(|| Error::Other("Could not get claim".to_string()))?;
        if claim.quality_adj_power <= BigInt::zero() {
            return Ok(false);
        }

        // No fee debt.
        if !miner_state.fee_debt().is_zero() {
            return Ok(false);
        }

        // No active consensus faults.
        let info = miner_state.info(self.db())?;
        if base_tipset.epoch() <= info.consensus_fault_elapsed {
            return Ok(false);
        }

        Ok(true)
    }

    pub async fn miner_get_base_info(
        &self,
        beacon_schedule: &BeaconSchedule,
        tipset: Tipset,
        addr: Address,
        epoch: ChainEpoch,
    ) -> anyhow::Result<Option<MiningBaseInfo>> {
        let prev_beacon = self
            .chain_store()
            .chain_index()
            .latest_beacon_entry(tipset.clone())?;

        let entries: Vec<BeaconEntry> = beacon_schedule
            .beacon_entries_for_block(
                self.chain_config().network_version(epoch),
                epoch,
                tipset.epoch(),
                &prev_beacon,
            )
            .await?;

        let base = entries.last().unwrap_or(&prev_beacon);

        let (lb_tipset, lb_state_root) = ChainStore::get_lookback_tipset_for_round(
            self.chain_index(),
            self.chain_config(),
            &tipset,
            epoch,
        )?;

        // If the miner actor doesn't exist in the current tipset, it is a
        // user-error and we must return an error message. If the miner exists
        // in the current tipset but not in the lookback tipset, we may not
        // error and should instead return None.
        let actor = self.get_required_actor(&addr, *tipset.parent_state())?;
        if self.get_actor(&addr, lb_state_root)?.is_none() {
            return Ok(None);
        }

        let miner_state = miner::State::load(self.db(), actor.code, actor.state)?;

        let addr_buf = to_vec(&addr)?;
        let rand = draw_randomness(
            base.signature(),
            DomainSeparationTag::WinningPoStChallengeSeed as i64,
            epoch,
            &addr_buf,
        )?;

        let network_version = self.chain_config().network_version(tipset.epoch());
        let sectors = self.get_sectors_for_winning_post(
            &lb_state_root,
            network_version,
            &addr,
            Randomness::new(rand.to_vec()),
        )?;

        if sectors.is_empty() {
            return Ok(None);
        }

        let (miner_power, total_power) = self
            .get_power(&lb_state_root, Some(&addr))?
            .context("failed to get power")?;

        let info = miner_state.info(self.db())?;

        let worker_key = self
            .resolve_to_deterministic_address(info.worker, &tipset)
            .await?;
        let eligible = self.eligible_to_mine(&addr, &tipset, &lb_tipset)?;

        Ok(Some(MiningBaseInfo {
            miner_power: miner_power.quality_adj_power,
            network_power: total_power.quality_adj_power,
            sectors,
            worker_key,
            sector_size: info.sector_size,
            prev_beacon_entry: prev_beacon,
            beacon_entries: entries,
            eligible_for_mining: eligible,
        }))
    }

    /// Checks power actor state for if miner meets consensus minimum
    /// requirements.
    pub fn miner_has_min_power(
        &self,
        policy: &Policy,
        addr: &Address,
        ts: &Tipset,
    ) -> anyhow::Result<bool> {
        let actor = self
            .get_actor(&Address::POWER_ACTOR, *ts.parent_state())?
            .ok_or_else(|| Error::state("Power actor address could not be resolved"))?;
        let ps = power::State::load(self.db(), actor.code, actor.state)?;

        ps.miner_nominal_power_meets_consensus_minimum(policy, self.db(), addr)
    }
}
