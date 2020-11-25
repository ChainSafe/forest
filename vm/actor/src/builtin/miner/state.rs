// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::DeadlineSectorMap;
use super::{
    assign_deadlines, deadline_is_mutable, deadlines::new_deadline_info, policy::*, Deadline,
};
use super::{types::*, Deadlines, PowerPair, Sectors, TerminationResult, VestingFunds};
use crate::{actor_assert, make_map_with_root, u64_key, ActorDowncast};
use address::Address;
use ahash::AHashSet;
use bitfield::BitField;
use cid::{Cid, Code::Blake2b256};
use clock::ChainEpoch;
use encoding::{serde_bytes, tuple::*, BytesDe, Cbor};
use fil_types::{
    deadlines::{DeadlineInfo, QuantSpec},
    NetworkVersion, RegisteredSealProof, SectorNumber, SectorSize, MAX_SECTOR_NUMBER,
};
use ipld_amt::Error as AmtError;
use ipld_blockstore::BlockStore;
use ipld_hamt::Error as HamtError;
use num_bigint::bigint_ser;
use num_traits::{Signed, Zero};
use std::{cmp, error::Error as StdError};
use vm::{actor_error, ActorError, ExitCode, TokenAmount};

/// Balance of Miner Actor should be greater than or equal to
/// the sum of PreCommitDeposits and LockedFunds.
/// It is possible for balance to fall below the sum of PCD, LF and
/// InitialPledgeRequirements, and this is a bad state (IP Debt)
/// that limits a miner actor's behavior (i.e. no balance withdrawals)
/// Excess balance as computed by st.GetAvailableBalance will be
/// withdrawable or usable for pre-commit deposit or pledge lock-up.
#[derive(Serialize_tuple, Deserialize_tuple, Clone)]
pub struct State {
    /// Contains static info about this miner
    pub info: Cid,

    /// Total funds locked as pre_commit_deposit
    #[serde(with = "bigint_ser")]
    pub pre_commit_deposits: TokenAmount,

    /// Total rewards and added funds locked in vesting table
    #[serde(with = "bigint_ser")]
    pub locked_funds: TokenAmount,

    /// VestingFunds (Vesting Funds schedule for the miner).
    pub vesting_funds: Cid,

    /// Sum of initial pledge requirements of all active sectors
    #[serde(with = "bigint_ser")]
    pub initial_pledge_requirement: TokenAmount,

    /// Sectors that have been pre-committed but not yet proven.
    /// Map, HAMT<SectorNumber, SectorPreCommitOnChainInfo>
    pub pre_committed_sectors: Cid,

    /// PreCommittedSectorsExpiry maintains the state required to expire PreCommittedSectors.
    pub pre_committed_sectors_expiry: Cid, // BitFieldQueue (AMT[Epoch]*BitField)

    /// Allocated sector IDs. Sector IDs can never be reused once allocated.
    pub allocated_sectors: Cid, // BitField

    /// Information for all proven and not-yet-garbage-collected sectors.
    ///
    /// Sectors are removed from this AMT when the partition to which the
    /// sector belongs is compacted.
    pub sectors: Cid, // Array, AMT[SectorNumber]SectorOnChainInfo (sparse)

    /// The first epoch in this miner's current proving period. This is the first epoch in which a PoSt for a
    /// partition at the miner's first deadline may arrive. Alternatively, it is after the last epoch at which
    /// a PoSt for the previous window is valid.
    /// Always greater than zero, this may be greater than the current epoch for genesis miners in the first
    /// WPoStProvingPeriod epochs of the chain; the epochs before the first proving period starts are exempt from Window
    /// PoSt requirements.
    /// Updated at the end of every period by a cron callback.
    pub proving_period_start: ChainEpoch,

    /// Index of the deadline within the proving period beginning at ProvingPeriodStart that has not yet been
    /// finalized.
    /// Updated at the end of each deadline window by a cron callback.
    pub current_deadline: u64,

    /// The sector numbers due for PoSt at each deadline in the current proving period, frozen at period start.
    /// New sectors are added and expired ones removed at proving period boundary.
    /// Faults are not subtracted from this in state, but on the fly.
    pub deadlines: Cid,

    /// Deadlines with outstanding fees for early sector termination.
    pub early_terminations: BitField,
}

impl Cbor for State {}

impl State {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        info_cid: Cid,
        period_start: ChainEpoch,
        empty_bitfield_cid: Cid,
        empty_array_cid: Cid,
        empty_map_cid: Cid,
        empty_deadlines_cid: Cid,
        empty_vesting_funds_cid: Cid,
    ) -> Self {
        Self {
            info: info_cid,

            pre_commit_deposits: TokenAmount::default(),
            locked_funds: TokenAmount::default(),

            vesting_funds: empty_vesting_funds_cid,

            initial_pledge_requirement: TokenAmount::default(),

            pre_committed_sectors: empty_map_cid,
            pre_committed_sectors_expiry: empty_array_cid,
            allocated_sectors: empty_bitfield_cid,
            sectors: empty_array_cid,
            proving_period_start: period_start,
            current_deadline: 0,
            deadlines: empty_deadlines_cid,
            early_terminations: BitField::new(),
        }
    }

    pub fn get_info<BS: BlockStore>(&self, store: &BS) -> Result<MinerInfo, Box<dyn StdError>> {
        match store.get(&self.info) {
            Ok(Some(info)) => Ok(info),
            Ok(None) => Err(actor_error!(ErrNotFound, "failed to get miner info").into()),
            Err(e) => Err(e.downcast_wrap("failed to get miner info")),
        }
    }

    pub fn save_info<BS: BlockStore>(
        &mut self,
        store: &BS,
        info: MinerInfo,
    ) -> Result<(), Box<dyn StdError>> {
        let cid = store.put(&info, Blake2b256)?;
        self.info = cid;
        Ok(())
    }

    /// Returns deadline calculations for the current (according to state) proving period.
    pub fn deadline_info(&self, current_epoch: ChainEpoch) -> DeadlineInfo {
        new_deadline_info(
            self.proving_period_start,
            self.current_deadline,
            current_epoch,
        )
    }

    /// Returns deadline calculations for the current (according to state) proving period.
    pub fn quant_spec_for_deadline(&self, deadline_idx: u64) -> QuantSpec {
        new_deadline_info(self.proving_period_start, deadline_idx, 0).quant_spec()
    }

    pub fn allocate_sector_number<BS: BlockStore>(
        &mut self,
        store: &BS,
        sector_number: SectorNumber,
    ) -> Result<(), ActorError> {
        // This will likely already have been checked, but this is a good place
        // to catch any mistakes.
        if sector_number > MAX_SECTOR_NUMBER {
            return Err(
                actor_error!(ErrIllegalArgument; "sector number out of range: {}", sector_number),
            );
        }

        let mut allocated_sectors: BitField = store
            .get(&self.allocated_sectors)
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to load allocated sectors bitfield",
                )
            })?
            .ok_or_else(|| actor_error!(ErrIllegalState, "allocated sectors bitfield not found"))?;

        if allocated_sectors.get(sector_number as usize) {
            return Err(
                actor_error!(ErrIllegalArgument; "sector number {} has already been allocated", sector_number),
            );
        }

        allocated_sectors.set(sector_number as usize);
        self.allocated_sectors = store.put(&allocated_sectors, Blake2b256).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalArgument,
                format!(
                    "failed to store allocated sectors bitfield after adding sector {}",
                    sector_number
                ),
            )
        })?;

        Ok(())
    }

    pub fn mask_sector_numbers<BS: BlockStore>(
        &mut self,
        store: &BS,
        sector_numbers: &BitField,
    ) -> Result<(), ActorError> {
        let last_sector_number = match sector_numbers.iter().last() {
            Some(sector_number) => sector_number as SectorNumber,
            None => return Err(actor_error!(ErrIllegalArgument; "invalid mask bitfield")),
        };

        #[allow(clippy::absurd_extreme_comparisons)]
        if last_sector_number > MAX_SECTOR_NUMBER {
            return Err(
                actor_error!(ErrIllegalArgument; "masked sector number %d exceeded max sector number"),
            );
        }

        let mut allocated_sectors: BitField = store
            .get(&self.allocated_sectors)
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to load allocated sectors bitfield",
                )
            })?
            .ok_or_else(|| {
                actor_error!(
                    ErrIllegalState,
                    "failed to load allocated sectors bitfield: does not exist"
                )
            })?;

        allocated_sectors |= sector_numbers;

        self.allocated_sectors = store.put(&allocated_sectors, Blake2b256).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalArgument,
                "failed to mask allocated sectors bitfield",
            )
        })?;

        Ok(())
    }

    pub fn put_precommitted_sector<BS: BlockStore>(
        &mut self,
        store: &BS,
        info: SectorPreCommitOnChainInfo,
    ) -> Result<(), HamtError> {
        let mut precommitted = make_map_with_root(&self.pre_committed_sectors, store)?;
        precommitted.set(u64_key(info.info.sector_number), info)?;

        self.pre_committed_sectors = precommitted.flush()?;
        Ok(())
    }

    pub fn get_precommitted_sector<BS: BlockStore>(
        &self,
        store: &BS,
        sector_num: SectorNumber,
    ) -> Result<Option<SectorPreCommitOnChainInfo>, HamtError> {
        let precommitted = make_map_with_root(&self.pre_committed_sectors, store)?;
        Ok(precommitted.get(&u64_key(sector_num))?.cloned())
    }

    /// Gets and returns the requested pre-committed sectors, skipping missing sectors.
    pub fn find_precommitted_sectors<BS: BlockStore>(
        &self,
        store: &BS,
        sector_numbers: &[SectorNumber],
    ) -> Result<Vec<SectorPreCommitOnChainInfo>, Box<dyn StdError>> {
        let precommitted = make_map_with_root::<_, SectorPreCommitOnChainInfo>(
            &self.pre_committed_sectors,
            store,
        )?;
        let mut result = Vec::with_capacity(sector_numbers.len());

        for &sector_number in sector_numbers {
            let info = match precommitted.get(&u64_key(sector_number)).map_err(|e| {
                e.downcast_wrap(format!(
                    "failed to load precommitment for {}",
                    sector_number
                ))
            })? {
                Some(info) => info.clone(),
                None => continue,
            };

            result.push(info);
        }

        Ok(result)
    }

    pub fn delete_precommitted_sectors<BS: BlockStore>(
        &mut self,
        store: &BS,
        sector_nums: &[SectorNumber],
    ) -> Result<(), HamtError> {
        let mut precommitted = make_map_with_root::<_, SectorPreCommitOnChainInfo>(
            &self.pre_committed_sectors,
            store,
        )?;

        for &sector_num in sector_nums {
            precommitted.delete(&u64_key(sector_num))?;
        }

        self.pre_committed_sectors = precommitted.flush()?;
        Ok(())
    }

    pub fn has_sector_number<BS: BlockStore>(
        &self,
        store: &BS,
        sector_num: SectorNumber,
    ) -> Result<bool, Box<dyn StdError>> {
        let sectors = Sectors::load(store, &self.sectors)?;
        Ok(sectors.get(sector_num)?.is_some())
    }

    pub fn put_sectors<BS: BlockStore>(
        &mut self,
        store: &BS,
        new_sectors: Vec<SectorOnChainInfo>,
    ) -> Result<(), Box<dyn StdError>> {
        let mut sectors = Sectors::load(store, &self.sectors)
            .map_err(|e| e.downcast_wrap("failed to load sectors"))?;

        sectors.store(new_sectors)?;

        self.sectors = sectors
            .amt
            .flush()
            .map_err(|e| e.downcast_wrap("failed to persist sectors"))?;

        Ok(())
    }

    pub fn get_sector<BS: BlockStore>(
        &self,
        store: &BS,
        sector_num: SectorNumber,
    ) -> Result<Option<SectorOnChainInfo>, Box<dyn StdError>> {
        let sectors = Sectors::load(store, &self.sectors)?;
        sectors.get(sector_num)
    }

    pub fn delete_sectors<BS: BlockStore>(
        &mut self,
        store: &BS,
        sector_nos: &BitField,
    ) -> Result<(), AmtError> {
        let mut sectors = Sectors::load(store, &self.sectors)?;

        for sector_num in sector_nos.iter() {
            sectors
                .amt
                .delete(sector_num as u64)
                .map_err(|e| e.downcast_wrap("could not delete sector number"))?;
        }

        self.sectors = sectors.amt.flush()?;
        Ok(())
    }

    pub fn for_each_sector<BS: BlockStore, F>(
        &self,
        store: &BS,
        mut f: F,
    ) -> Result<(), Box<dyn StdError>>
    where
        F: FnMut(&SectorOnChainInfo) -> Result<(), Box<dyn StdError>>,
    {
        let sectors = Sectors::load(store, &self.sectors)?;
        sectors.amt.for_each(|_, v| f(&v))
    }

    /// Returns the deadline and partition index for a sector number.
    pub fn find_sector<BS: BlockStore>(
        &self,
        store: &BS,
        sector_number: SectorNumber,
    ) -> Result<(u64, u64), Box<dyn StdError>> {
        let deadlines = self.load_deadlines(store)?;
        Ok(deadlines.find_sector(store, sector_number)?)
    }

    /// Schedules each sector to expire at its next deadline end. If it can't find
    /// any given sector, it skips it.
    ///
    /// This method assumes that each sector's power has not changed, despite the rescheduling.
    ///
    /// Note: this method is used to "upgrade" sectors, rescheduling the now-replaced
    /// sectors to expire at the end of the next deadline. Given the expense of
    /// sealing a sector, this function skips missing/faulty/terminated "upgraded"
    /// sectors instead of failing. That way, the new sectors can still be proved.
    pub fn reschedule_sector_expirations<BS: BlockStore>(
        &mut self,
        store: &BS,
        current_epoch: ChainEpoch,
        sector_size: SectorSize,
        mut deadline_sectors: DeadlineSectorMap,
    ) -> Result<(), Box<dyn StdError>> {
        let mut deadlines = self.load_deadlines(store)?;
        let sectors = Sectors::load(store, &self.sectors)?;

        for (deadline_idx, partition_sectors) in deadline_sectors.iter() {
            let deadline_info =
                new_deadline_info(self.proving_period_start, deadline_idx, current_epoch)
                    .next_not_elapsed();
            let new_expiration = deadline_info.last();
            let mut deadline = deadlines.load_deadline(store, deadline_idx)?;

            deadline.reschedule_sector_expirations(
                store,
                &sectors,
                new_expiration,
                partition_sectors,
                sector_size,
                deadline_info.quant_spec(),
            )?;

            deadlines.update_deadline(store, deadline_idx, &deadline)?;
        }

        self.save_deadlines(store, deadlines)?;

        Ok(())
    }

    /// Assign new sectors to deadlines.
    pub fn assign_sectors_to_deadlines<BS: BlockStore>(
        &mut self,
        store: &BS,
        current_epoch: ChainEpoch,
        mut sectors: Vec<SectorOnChainInfo>,
        partition_size: u64,
        sector_size: SectorSize,
    ) -> Result<PowerPair, Box<dyn StdError>> {
        let mut deadlines = self.load_deadlines(store)?;

        // Sort sectors by number to get better runs in partition bitfields.
        sectors.sort_by_key(|info| info.sector_number);

        let mut deadline_vec: Vec<Option<Deadline>> =
            (0..WPOST_PERIOD_DEADLINES).map(|_| None).collect();

        deadlines.for_each(store, |deadline_idx, deadline| {
            // Skip deadlines that aren't currently mutable.
            if deadline_is_mutable(self.proving_period_start, deadline_idx, current_epoch) {
                deadline_vec[deadline_idx as usize] = Some(deadline);
            }

            Ok(())
        })?;

        let mut new_power = PowerPair::zero();

        for (deadline_idx, deadline_sectors) in
            assign_deadlines(partition_size, &deadline_vec, sectors)
                .into_iter()
                .enumerate()
        {
            if deadline_sectors.is_empty() {
                continue;
            }

            let quant = self.quant_spec_for_deadline(deadline_idx as u64);
            let deadline = deadline_vec[deadline_idx].as_mut().unwrap();

            let deadline_new_power = deadline.add_sectors(
                store,
                partition_size,
                &deadline_sectors,
                sector_size,
                quant,
            )?;

            new_power += &deadline_new_power;

            deadlines.update_deadline(store, deadline_idx as u64, deadline)?;
        }

        self.save_deadlines(store, deadlines)?;

        Ok(new_power)
    }

    /// Pops up to `max_sectors` early terminated sectors from all deadlines.
    ///
    /// Returns `true` if we still have more early terminations to process.
    pub fn pop_early_terminations<BS: BlockStore>(
        &mut self,
        store: &BS,
        max_partitions: u64,
        max_sectors: u64,
    ) -> Result<(TerminationResult, /* has more */ bool), Box<dyn StdError>> {
        // Anything to do? This lets us avoid loading the deadlines if there's nothing to do.
        if self.early_terminations.is_empty() {
            return Ok((Default::default(), false));
        }

        // Load deadlines
        let mut deadlines = self.load_deadlines(store)?;

        let mut result = TerminationResult::new();
        let mut to_unset = Vec::new();

        // Process early terminations.
        for i in self.early_terminations.iter() {
            let deadline_idx = i as u64;

            // Load deadline + partitions.
            let mut deadline = deadlines.load_deadline(store, deadline_idx)?;

            let (deadline_result, more) = deadline
                .pop_early_terminations(
                    store,
                    max_partitions - result.partitions_processed,
                    max_sectors - result.sectors_processed,
                )
                .map_err(|e| {
                    e.downcast_wrap(format!(
                        "failed to pop early terminations for deadline {}",
                        deadline_idx
                    ))
                })?;

            result += deadline_result;

            if !more {
                to_unset.push(i);
            }

            // Save the deadline
            deadlines.update_deadline(store, deadline_idx, &deadline)?;

            if !result.below_limit(max_partitions, max_sectors) {
                break;
            }
        }

        for deadline_idx in to_unset {
            self.early_terminations.unset(deadline_idx);
        }

        // Save back the deadlines.
        self.save_deadlines(store, deadlines)?;

        // Ok, check to see if we've handled all early terminations.
        let no_early_terminations = self.early_terminations.is_empty();

        Ok((result, !no_early_terminations))
    }

    // /Returns an error if the target sector cannot be found and/or is faulty/terminated.
    pub fn check_sector_health<BS: BlockStore>(
        &self,
        store: &BS,
        deadline_idx: u64,
        partition_idx: u64,
        sector_number: SectorNumber,
    ) -> Result<(), Box<dyn StdError>> {
        let deadlines = self.load_deadlines(store)?;
        let deadline = deadlines.load_deadline(store, deadline_idx)?;
        let partition = deadline.load_partition(store, partition_idx)?;

        if !partition.sectors.get(sector_number as usize) {
            return Err(actor_error!(
                ErrNotFound;
                "sector {} not a member of partition {}, deadline {}",
                sector_number, partition_idx, deadline_idx
            )
            .into());
        }

        if partition.faults.get(sector_number as usize) {
            return Err(actor_error!(
                ErrForbidden;
                "sector {} not a member of partition {}, deadline {}",
                sector_number, partition_idx, deadline_idx
            )
            .into());
        }

        if partition.terminated.get(sector_number as usize) {
            return Err(actor_error!(
                ErrNotFound;
                "sector {} not of partition {}, deadline {} is terminated",
                sector_number, partition_idx, deadline_idx
            )
            .into());
        }

        Ok(())
    }

    /// Loads sector info for a sequence of sectors.
    pub fn load_sector_infos<BS: BlockStore>(
        &self,
        store: &BS,
        sectors: &BitField,
    ) -> Result<Vec<SectorOnChainInfo>, Box<dyn StdError>> {
        Ok(Sectors::load(store, &self.sectors)?.load_sector(sectors)?)
    }

    /// Loads info for a set of sectors to be proven.
    /// If any of the sectors are declared faulty and not to be recovered, info for the first non-faulty sector is substituted instead.
    /// If any of the sectors are declared recovered, they are returned from this method.
    pub fn load_sector_infos_for_proof<BS: BlockStore>(
        &mut self,
        store: &BS,
        proven_sectors: &BitField,
        expected_faults: &BitField,
    ) -> Result<Vec<SectorOnChainInfo>, Box<dyn StdError>> {
        let non_faults = proven_sectors - expected_faults;

        if non_faults.is_empty() {
            return Ok(Vec::new());
        }

        // Select a non-faulty sector as a substitute for faulty ones.
        let good_sector_no = non_faults
            .first()
            .ok_or("no non-faulty sectors in partitions")?;

        // load sector infos
        let sector_infos = self.load_sector_infos_with_fault_mask(
            store,
            &proven_sectors,
            &expected_faults,
            good_sector_no as u64,
        )?;

        Ok(sector_infos)
    }

    /// Loads sector info for a sequence of sectors, substituting info for a stand-in sector for any that are faulty.
    fn load_sector_infos_with_fault_mask<BS: BlockStore>(
        &self,
        store: &BS,
        sectors_bf: &BitField,
        faults: &BitField,
        fault_stand_in: SectorNumber,
    ) -> Result<Vec<SectorOnChainInfo>, Box<dyn StdError>> {
        let sectors = Sectors::load(store, &self.sectors)
            .map_err(|e| e.downcast_wrap("failed to load sectors array"))?;

        let stand_in_info = sectors.must_get(fault_stand_in).map_err(|e| {
            e.downcast_wrap(format!("failed to load stand-in sector {}", fault_stand_in))
        })?;

        // Expand faults into a map for quick lookups.
        // The faults bitfield should already be a subset of the sectors bitfield.
        let fault_max = sectors.amt.count();
        let fault_set: AHashSet<_> = faults.bounded_iter(fault_max as usize)?.collect();

        // Load the sector infos, masking out fault sectors with a good one.
        let mut sector_infos: Vec<SectorOnChainInfo> = Vec::new();
        for i in sectors_bf.iter() {
            let sector = if fault_set.contains(&i) {
                stand_in_info.clone()
            } else {
                sectors
                    .must_get(i as u64)
                    .map_err(|e| e.downcast_wrap(format!("failed to load sector {}", i)))?
            };

            sector_infos.push(sector);
        }

        Ok(sector_infos)
    }

    pub fn load_deadlines<BS: BlockStore>(&self, store: &BS) -> Result<Deadlines, ActorError> {
        store
            .get::<Deadlines>(&self.deadlines)
            .ok()
            .flatten()
            .ok_or_else(
                || actor_error!(ErrIllegalState; "failed to load deadlines {}", self.deadlines),
            )
    }

    pub fn save_deadlines<BS: BlockStore>(
        &mut self,
        store: &BS,
        deadlines: Deadlines,
    ) -> Result<(), Box<dyn StdError>> {
        self.deadlines = store.put(&deadlines, Blake2b256)?;
        Ok(())
    }

    /// Loads the vesting funds table from the store.
    pub fn load_vesting_funds<BS: BlockStore>(
        &self,
        store: &BS,
    ) -> Result<VestingFunds, Box<dyn StdError>> {
        Ok(store
            .get(&self.vesting_funds)
            .map_err(|e| {
                e.downcast_wrap(
                    format!("failed to load vesting funds {}", self.vesting_funds),
                )
            })?
            .ok_or_else(|| actor_error!(ErrNotFound; "failed to load vesting funds {:?}", self.vesting_funds))?)
    }

    /// Saves the vesting table to the store.
    pub fn save_vesting_funds<BS: BlockStore>(
        &mut self,
        store: &BS,
        funds: &VestingFunds,
    ) -> Result<(), Box<dyn StdError>> {
        self.vesting_funds = store.put(funds, Blake2b256)?;
        Ok(())
    }

    //
    // Funds and vesting
    //

    pub fn add_pre_commit_deposit(&mut self, amount: &TokenAmount) {
        let new_total = &self.pre_commit_deposits + amount;
        assert!(
            !new_total.is_negative(),
            "negative pre-commit deposit {} after adding {} to prior {}",
            new_total,
            amount,
            self.pre_commit_deposits
        );
        self.pre_commit_deposits = new_total;
    }

    pub fn add_initial_pledge_requirement(&mut self, amount: &TokenAmount) {
        let new_total = &self.initial_pledge_requirement + amount;
        assert!(
            !new_total.is_negative(),
            "negative initial pledge requirement {} after adding {} to prior {}",
            new_total,
            amount,
            self.initial_pledge_requirement
        );
        self.initial_pledge_requirement = new_total;
    }

    /// First vests and unlocks the vested funds AND then locks the given funds in the vesting table.
    pub fn add_locked_funds<BS: BlockStore>(
        &mut self,
        store: &BS,
        current_epoch: ChainEpoch,
        vesting_sum: &TokenAmount,
        spec: VestSpec,
    ) -> Result<TokenAmount, Box<dyn StdError>> {
        assert!(
            !vesting_sum.is_negative(),
            "negative vesting sum {}",
            vesting_sum
        );

        let mut vesting_funds = self.load_vesting_funds(store)?;

        // unlock vested funds first
        let amount_unlocked = vesting_funds.unlock_vested_funds(current_epoch);
        self.locked_funds -= &amount_unlocked;

        // add locked funds now
        vesting_funds.add_locked_funds(current_epoch, vesting_sum, self.proving_period_start, spec);
        self.locked_funds += vesting_sum;
        assert!(!self.locked_funds.is_negative());

        // save the updated vesting table state
        self.save_vesting_funds(store, &vesting_funds)?;

        Ok(amount_unlocked)
    }

    /// First unlocks unvested funds from the vesting table. If the target is not yet hit it deducts
    /// funds from the (new) available balance. Returns the amount unlocked from the vesting table
    /// and the amount taken from current balance. If the penalty exceeds the total amount available
    /// in the vesting table and unlocked funds the penalty is reduced to match. This must be fixed
    /// when handling bankrupcy:
    /// https://github.com/filecoin-project/specs-actors/issues/627
    pub fn penalize_funds_in_priority_order<BS: BlockStore>(
        &mut self,
        store: &BS,
        current_epoch: ChainEpoch,
        target: &TokenAmount,
        unlocked_balance: &TokenAmount,
    ) -> Result<
        (
            TokenAmount, // from vesting
            TokenAmount, // from balance
        ),
        Box<dyn StdError>,
    > {
        let from_vesting = self.unlock_unvested_funds(store, current_epoch, &target)?;

        if from_vesting == *target {
            return Ok((from_vesting, TokenAmount::zero()));
        }

        // unlocked funds were just deducted from available, so track that
        let remaining = target - &from_vesting;

        let from_balance = cmp::min(unlocked_balance, &remaining).clone();
        Ok((from_vesting, from_balance))
    }

    /// Unlocks an amount of funds that have *not yet vested*, if possible.
    /// The soonest-vesting entries are unlocked first.
    /// Returns the amount actually unlocked.
    pub fn unlock_unvested_funds<BS: BlockStore>(
        &mut self,
        store: &BS,
        current_epoch: ChainEpoch,
        target: &TokenAmount,
    ) -> Result<TokenAmount, Box<dyn StdError>> {
        let mut vesting_funds = self.load_vesting_funds(store)?;
        let amount_unlocked = vesting_funds.unlock_unvested_funds(current_epoch, target);
        self.locked_funds -= &amount_unlocked;
        assert!(!self.locked_funds.is_negative());

        self.save_vesting_funds(store, &vesting_funds)?;
        Ok(amount_unlocked)
    }

    /// Unlocks all vesting funds that have vested before the provided epoch.
    /// Returns the amount unlocked.
    pub fn unlock_vested_funds<BS: BlockStore>(
        &mut self,
        store: &BS,
        current_epoch: ChainEpoch,
    ) -> Result<TokenAmount, Box<dyn StdError>> {
        let mut vesting_funds = self.load_vesting_funds(store)?;
        let amount_unlocked = vesting_funds.unlock_vested_funds(current_epoch);
        self.locked_funds -= &amount_unlocked;
        assert!(!self.locked_funds.is_negative());

        self.save_vesting_funds(store, &vesting_funds)?;
        Ok(amount_unlocked)
    }

    /// CheckVestedFunds returns the amount of vested funds that have vested before the provided epoch.
    pub fn check_vested_funds<BS: BlockStore>(
        &self,
        store: &BS,
        current_epoch: ChainEpoch,
    ) -> Result<TokenAmount, Box<dyn StdError>> {
        let vesting_funds = self.load_vesting_funds(store)?;
        Ok(vesting_funds
            .funds
            .iter()
            .take_while(|fund| fund.epoch < current_epoch)
            .fold(TokenAmount::zero(), |acc, fund| acc + &fund.amount))
    }

    /// Unclaimed funds that are not locked -- includes funds used to cover initial pledge requirement.
    pub fn get_unlocked_balance(
        &self,
        actor_balance: &TokenAmount,
        network_version: NetworkVersion,
    ) -> Result<TokenAmount, ActorError> {
        let unlocked_balance = actor_balance - &self.locked_funds - &self.pre_commit_deposits;
        actor_assert(
            unlocked_balance >= TokenAmount::zero(),
            network_version,
            "Unlocked balance cannot be less than zero",
        )?;
        Ok(unlocked_balance)
    }

    /// Unclaimed funds. Actor balance - (locked funds, precommit deposit, ip requirement)
    /// Can go negative if the miner is in IP debt.
    pub fn get_available_balance(
        &self,
        actor_balance: &TokenAmount,
        network_version: NetworkVersion,
    ) -> Result<TokenAmount, ActorError> {
        // (actor_balance - &self.locked_funds) - &self.pre_commit_deposit
        Ok(self.get_unlocked_balance(actor_balance, network_version)?
            - &self.initial_pledge_requirement)
    }

    pub fn assert_balance_invariants(
        &self,
        balance: &TokenAmount,
        network_version: NetworkVersion,
    ) -> Result<(), ActorError> {
        actor_assert(
            self.pre_commit_deposits >= TokenAmount::zero(),
            network_version,
            "assert balance invariant, pre commit deposits < 0",
        )?;
        actor_assert(
            self.locked_funds >= TokenAmount::zero(),
            network_version,
            "assert balance invariant, locked funds < 0",
        )?;
        actor_assert(
            *balance >= &self.pre_commit_deposits + &self.locked_funds,
            network_version,
            "assert balance invariant, balance < pcd + lf",
        )?;

        Ok(())
    }

    pub fn meets_initial_pledge_condition(
        &self,
        balance: &TokenAmount,
        network_version: NetworkVersion,
    ) -> Result<bool, ActorError> {
        Ok(self.get_unlocked_balance(balance, network_version)? >= self.initial_pledge_requirement)
    }

    /// pre-commit expiry
    pub fn quant_spec_every_deadline(&self) -> QuantSpec {
        QuantSpec {
            unit: WPOST_CHALLENGE_WINDOW,
            offset: self.proving_period_start,
        }
    }

    pub fn add_pre_commit_expiry<BS: BlockStore>(
        &mut self,
        store: &BS,
        expire_epoch: ChainEpoch,
        sector_number: SectorNumber,
    ) -> Result<(), Box<dyn StdError>> {
        // Load BitField Queue for sector expiry
        let quant = self.quant_spec_every_deadline();
        let mut queue = super::BitFieldQueue::new(store, &self.pre_committed_sectors_expiry, quant)
            .map_err(|e| e.downcast_wrap("failed to load pre-commit sector queue"))?;

        // add entry for this sector to the queue
        queue.add_to_queue_values(expire_epoch, &[sector_number])?;
        self.pre_committed_sectors_expiry = queue.amt.flush()?;

        Ok(())
    }

    pub fn check_precommit_expiry<BS: BlockStore>(
        &mut self,
        store: &BS,
        sectors: &BitField,
        network_version: NetworkVersion,
    ) -> Result<TokenAmount, Box<dyn StdError>> {
        let mut deposit_to_burn = TokenAmount::zero();
        let mut precommits_to_delete = Vec::new();

        for i in sectors.iter() {
            let sector_number = i as SectorNumber;

            let sector = match self.get_precommitted_sector(store, sector_number)? {
                Some(sector) => sector,
                // already committed/deleted
                None => continue,
            };

            // mark it for deletion
            precommits_to_delete.push(sector_number);

            // increment deposit to burn
            deposit_to_burn += sector.pre_commit_deposit;
        }

        // Actually delete it.
        if !precommits_to_delete.is_empty() {
            self.delete_precommitted_sectors(store, &precommits_to_delete)?;
        }

        self.pre_commit_deposits -= &deposit_to_burn;
        actor_assert(
            self.pre_commit_deposits >= TokenAmount::zero(),
            network_version,
            "check precommit expiry deposits < 0",
        )?;

        Ok(deposit_to_burn)
    }
}

/// Static information about miner
#[derive(Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct MinerInfo {
    /// Account that owns this miner
    /// - Income and returned collateral are paid to this address
    /// - This address is also allowed to change the worker address for the miner
    pub owner: Address,

    /// Worker account for this miner
    /// This will be the key that is used to sign blocks created by this miner, and
    /// sign messages sent on behalf of this miner to commit sectors, submit PoSts, and
    /// other day to day miner activities
    pub worker: Address,

    /// Additional addresses that are permitted to submit messages controlling this actor (optional).
    pub control_addresses: Vec<Address>, // Must all be ID addresses.

    /// Optional worker key to update at an epoch
    pub pending_worker_key: Option<WorkerKeyChange>,

    /// Libp2p identity that should be used when connecting to this miner
    #[serde(with = "serde_bytes")]
    pub peer_id: Vec<u8>,

    /// Vector of byte arrays representing Libp2p multi-addresses used for establishing a connection with this miner.
    pub multi_address: Vec<BytesDe>,

    /// The proof type used by this miner for sealing sectors.
    pub seal_proof_type: RegisteredSealProof,

    /// Amount of space in each sector committed to the network by this miner
    pub sector_size: SectorSize,

    /// The number of sectors in each Window PoSt partition (proof).
    /// This is computed from the proof type and represented here redundantly.
    pub window_post_partition_sectors: u64,
}

impl MinerInfo {
    pub fn new(
        owner: Address,
        worker: Address,
        control_addresses: Vec<Address>,
        peer_id: Vec<u8>,
        multi_address: Vec<BytesDe>,
        seal_proof_type: RegisteredSealProof,
    ) -> Result<Self, String> {
        let sector_size = seal_proof_type.sector_size()?;
        let window_post_partition_sectors = seal_proof_type.window_post_partitions_sector()?;

        Ok(Self {
            owner,
            worker,
            control_addresses,
            pending_worker_key: None,
            peer_id,
            multi_address,
            seal_proof_type,
            sector_size,
            window_post_partition_sectors,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding::{from_slice, to_vec};
    use libp2p::PeerId;

    #[test]
    fn miner_info_serialize() {
        let info = MinerInfo {
            owner: Address::new_id(2),
            worker: Address::new_id(3),
            control_addresses: vec![Address::new_id(4), Address::new_id(5)],
            pending_worker_key: None,
            peer_id: PeerId::random().into_bytes(),
            multi_address: vec![BytesDe(PeerId::random().into_bytes())],
            sector_size: SectorSize::_2KiB,
            seal_proof_type: RegisteredSealProof::from(1),
            window_post_partition_sectors: 0,
        };
        let bz = to_vec(&info).unwrap();
        assert_eq!(from_slice::<MinerInfo>(&bz).unwrap(), info);
    }
}
