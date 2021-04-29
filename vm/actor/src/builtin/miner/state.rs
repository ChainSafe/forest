// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{
    assign_deadlines, deadline_is_mutable, deadlines::new_deadline_info,
    new_deadline_info_from_offset_and_epoch, policy::*, quant_spec_for_deadline, types::*,
    BitFieldQueue, Deadline, DeadlineSectorMap, Deadlines, PowerPair, Sectors, TerminationResult,
    VestingFunds,
};
use crate::{make_empty_map, make_map_with_root_and_bitwidth, u64_key, ActorDowncast};
use address::Address;
use bitfield::BitField;
use cid::{Cid, Code::Blake2b256};
use clock::{ChainEpoch, EPOCH_UNDEFINED};
use encoding::{serde_bytes, tuple::*, BytesDe, Cbor};
use fil_types::{
    deadlines::{DeadlineInfo, QuantSpec},
    RegisteredPoStProof, SectorNumber, SectorSize, HAMT_BIT_WIDTH, MAX_SECTOR_NUMBER,
};
use ipld_amt::{Amt, Error as AmtError};
use ipld_blockstore::BlockStore;
use ipld_hamt::Error as HamtError;
use num_bigint::bigint_ser;
use num_traits::{Signed, Zero};
use std::ops::Neg;
use std::{cmp, error::Error as StdError};
use vm::{actor_error, ActorError, ExitCode, TokenAmount};

const PRECOMMIT_EXPIRY_AMT_BITWIDTH: usize = 6;
const SECTORS_AMT_BITWIDTH: usize = 5;

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

    /// Absolute value of debt this miner owes from unpaid fees.
    #[serde(with = "bigint_ser")]
    pub fee_debt: TokenAmount,

    /// Sum of initial pledge requirements of all active sectors
    #[serde(with = "bigint_ser")]
    pub initial_pledge: TokenAmount,

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
    pub current_deadline: usize,

    /// The sector numbers due for PoSt at each deadline in the current proving period, frozen at period start.
    /// New sectors are added and expired ones removed at proving period boundary.
    /// Faults are not subtracted from this in state, but on the fly.
    pub deadlines: Cid,

    /// Deadlines with outstanding fees for early sector termination.
    pub early_terminations: BitField,

    // True when miner cron is active, false otherwise
    pub deadline_cron_active: bool,
}

impl Cbor for State {}

impl State {
    #[allow(clippy::too_many_arguments)]
    pub fn new<BS: BlockStore>(
        store: &BS,
        info_cid: Cid,
        period_start: ChainEpoch,
        deadline_idx: usize,
    ) -> Result<Self, Box<dyn StdError>> {
        let empty_precommit_map = make_empty_map::<_, ()>(store, HAMT_BIT_WIDTH)
            .flush()
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to construct empty precommit map",
                )
            })?;
        let empty_precommits_expiry_array =
            Amt::<BitField, BS>::new_with_bit_width(store, PRECOMMIT_EXPIRY_AMT_BITWIDTH)
                .flush()
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        "failed to construct empty precommits array",
                    )
                })?;
        let empty_sectors_array =
            Amt::<SectorOnChainInfo, BS>::new_with_bit_width(store, SECTORS_AMT_BITWIDTH)
                .flush()
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        "failed to construct sectors array",
                    )
                })?;
        let empty_bitfield = store.put(&BitField::new(), Blake2b256).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                "failed to construct empty bitfield",
            )
        })?;
        let deadline = Deadline::new(store)?;
        let empty_deadline = store.put(&deadline, Blake2b256).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                "failed to construct illegal state",
            )
        })?;

        let empty_deadlines = store
            .put(&Deadlines::new(empty_deadline), Blake2b256)
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to construct illegal state",
                )
            })?;

        let empty_vesting_funds_cid = store.put(&VestingFunds::new(), Blake2b256).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                "failed to construct illegal state",
            )
        })?;

        Ok(Self {
            info: info_cid,

            pre_commit_deposits: TokenAmount::default(),
            locked_funds: TokenAmount::default(),

            vesting_funds: empty_vesting_funds_cid,

            initial_pledge: TokenAmount::default(),
            fee_debt: TokenAmount::default(),

            pre_committed_sectors: empty_precommit_map,
            pre_committed_sectors_expiry: empty_precommits_expiry_array,
            allocated_sectors: empty_bitfield,
            sectors: empty_sectors_array,
            proving_period_start: period_start,
            current_deadline: deadline_idx,
            deadlines: empty_deadlines,
            early_terminations: BitField::new(),
            deadline_cron_active: false,
        })
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
        info: &MinerInfo,
    ) -> Result<(), Box<dyn StdError>> {
        let cid = store.put(&info, Blake2b256)?;
        self.info = cid;
        Ok(())
    }

    /// Returns deadline calculations for the current (according to state) proving period.
    pub fn deadline_info(&self, current_epoch: ChainEpoch) -> DeadlineInfo {
        new_deadline_info_from_offset_and_epoch(self.proving_period_start, current_epoch)
    }
    // Returns deadline calculations for the state recorded proving period and deadline.
    // This is out of date if the a miner does not have an active miner cron
    pub fn recorded_deadline_info(&self, current_epoch: ChainEpoch) -> DeadlineInfo {
        new_deadline_info(
            self.proving_period_start,
            self.current_deadline,
            current_epoch,
        )
    }

    // Returns current proving period start for the current epoch according to the current epoch and constant state offset
    pub fn current_proving_period_start(&self, current_epoch: ChainEpoch) -> ChainEpoch {
        let dl_info = self.deadline_info(current_epoch);
        dl_info.period_start
    }

    /// Returns deadline calculations for the current (according to state) proving period.
    pub fn quant_spec_for_deadline(&self, deadline_idx: usize) -> QuantSpec {
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

    /// Stores a pre-committed sector info, failing if the sector number is already present.
    pub fn put_precommitted_sector<BS: BlockStore>(
        &mut self,
        store: &BS,
        info: SectorPreCommitOnChainInfo,
    ) -> Result<(), Box<dyn StdError>> {
        let mut precommitted =
            make_map_with_root_and_bitwidth(&self.pre_committed_sectors, store, HAMT_BIT_WIDTH)?;
        let sector_number = info.info.sector_number;
        let modified = precommitted
            .set_if_absent(u64_key(sector_number), info)
            .map_err(|e| {
                e.downcast_wrap(format!(
                    "failed to store pre-commitment for {:?}",
                    sector_number
                ))
            })?;
        if !modified {
            return Err(format!("sector {} already pre-commited", sector_number).into());
        }
        self.pre_committed_sectors = precommitted.flush()?;
        Ok(())
    }

    pub fn get_precommitted_sector<BS: BlockStore>(
        &self,
        store: &BS,
        sector_num: SectorNumber,
    ) -> Result<Option<SectorPreCommitOnChainInfo>, HamtError> {
        let precommitted =
            make_map_with_root_and_bitwidth(&self.pre_committed_sectors, store, HAMT_BIT_WIDTH)?;
        Ok(precommitted.get(&u64_key(sector_num))?.cloned())
    }

    /// Gets and returns the requested pre-committed sectors, skipping missing sectors.
    pub fn find_precommitted_sectors<BS: BlockStore>(
        &self,
        store: &BS,
        sector_numbers: &[SectorNumber],
    ) -> Result<Vec<SectorPreCommitOnChainInfo>, Box<dyn StdError>> {
        let precommitted = make_map_with_root_and_bitwidth::<_, SectorPreCommitOnChainInfo>(
            &self.pre_committed_sectors,
            store,
            HAMT_BIT_WIDTH,
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
        let mut precommitted = make_map_with_root_and_bitwidth::<_, SectorPreCommitOnChainInfo>(
            &self.pre_committed_sectors,
            store,
            HAMT_BIT_WIDTH,
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
                .delete(sector_num)
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
    ) -> Result<(usize, usize), Box<dyn StdError>> {
        let deadlines = self.load_deadlines(store)?;
        deadlines.find_sector(store, sector_number)
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
    ) -> Result<Vec<SectorOnChainInfo>, Box<dyn StdError>> {
        let mut deadlines = self.load_deadlines(store)?;
        let sectors = Sectors::load(store, &self.sectors)?;

        let mut all_replaced = Vec::new();
        for (deadline_idx, partition_sectors) in deadline_sectors.iter() {
            let deadline_info = new_deadline_info(
                self.current_proving_period_start(current_epoch),
                deadline_idx,
                current_epoch,
            )
            .next_not_elapsed();
            let new_expiration = deadline_info.last();
            let mut deadline = deadlines.load_deadline(store, deadline_idx)?;

            let replaced = deadline.reschedule_sector_expirations(
                store,
                &sectors,
                new_expiration,
                partition_sectors,
                sector_size,
                deadline_info.quant_spec(),
            )?;
            all_replaced.extend(replaced);

            deadlines.update_deadline(store, deadline_idx, &deadline)?;
        }

        self.save_deadlines(store, deadlines)?;

        Ok(all_replaced)
    }

    /// Assign new sectors to deadlines.
    pub fn assign_sectors_to_deadlines<BS: BlockStore>(
        &mut self,
        store: &BS,
        current_epoch: ChainEpoch,
        mut sectors: Vec<SectorOnChainInfo>,
        partition_size: u64,
        sector_size: SectorSize,
    ) -> Result<(), Box<dyn StdError>> {
        let mut deadlines = self.load_deadlines(store)?;

        // Sort sectors by number to get better runs in partition bitfields.
        sectors.sort_by_key(|info| info.sector_number);

        let mut deadline_vec: Vec<Option<Deadline>> =
            (0..WPOST_PERIOD_DEADLINES).map(|_| None).collect();

        deadlines.for_each(store, |deadline_idx, deadline| {
            // Skip deadlines that aren't currently mutable.
            if deadline_is_mutable(
                self.current_proving_period_start(current_epoch),
                deadline_idx,
                current_epoch,
            ) {
                deadline_vec[deadline_idx as usize] = Some(deadline);
            }

            Ok(())
        })?;

        let deadline_to_sectors = assign_deadlines(
            MAX_PARTITIONS_PER_DEADLINE,
            partition_size,
            &deadline_vec,
            sectors,
        )?;

        for (deadline_idx, deadline_sectors) in deadline_to_sectors.into_iter().enumerate() {
            if deadline_sectors.is_empty() {
                continue;
            }

            let quant = self.quant_spec_for_deadline(deadline_idx);
            let deadline = deadline_vec[deadline_idx].as_mut().unwrap();

            // The power returned from AddSectors is ignored because it's not activated (proven) yet.
            let proven = false;
            deadline.add_sectors(
                store,
                partition_size,
                proven,
                &deadline_sectors,
                sector_size,
                quant,
            )?;

            deadlines.update_deadline(store, deadline_idx, deadline)?;
        }

        self.save_deadlines(store, deadlines)?;

        Ok(())
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
            let deadline_idx = i;

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
        deadline_idx: usize,
        partition_idx: usize,
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

    pub fn load_deadlines<BS: BlockStore>(&self, store: &BS) -> Result<Deadlines, ActorError> {
        store
            .get::<Deadlines>(&self.deadlines)
            .map_err(|e| e.downcast_default(ExitCode::ErrIllegalState, "failed to load deadlines"))?
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

    // Return true when the miner actor needs to continue scheduling deadline crons
    pub fn continue_deadline_cron(&self) -> bool {
        !self.pre_commit_deposits.is_zero()
            || !self.initial_pledge.is_zero()
            || !self.locked_funds.is_zero()
    }

    //
    // Funds and vesting
    //

    pub fn add_pre_commit_deposit(&mut self, amount: &TokenAmount) -> Result<(), String> {
        let new_total = &self.pre_commit_deposits + amount;
        if new_total.is_negative() {
            return Err(format!(
                "negative pre-commit deposit {} after adding {} to prior {}",
                new_total, amount, self.pre_commit_deposits
            ));
        }
        self.pre_commit_deposits = new_total;
        Ok(())
    }

    pub fn add_initial_pledge(&mut self, amount: &TokenAmount) -> Result<(), String> {
        let new_total = &self.initial_pledge + amount;
        if new_total.is_negative() {
            return Err(format!(
                "negative initial pledge requirement {} after adding {} to prior {}",
                new_total, amount, self.initial_pledge
            ));
        }
        self.initial_pledge = new_total;
        Ok(())
    }

    pub fn apply_penalty(&mut self, penalty: &TokenAmount) -> Result<(), String> {
        if penalty.is_negative() {
            Err(format!("applying negative penalty {} not allowed", penalty))
        } else {
            self.fee_debt += penalty;
            Ok(())
        }
    }

    /// First vests and unlocks the vested funds AND then locks the given funds in the vesting table.
    pub fn add_locked_funds<BS: BlockStore>(
        &mut self,
        store: &BS,
        current_epoch: ChainEpoch,
        vesting_sum: &TokenAmount,
        spec: &VestSpec,
    ) -> Result<TokenAmount, Box<dyn StdError>> {
        if vesting_sum.is_negative() {
            return Err(format!("negative vesting sum {}", vesting_sum).into());
        }

        let mut vesting_funds = self.load_vesting_funds(store)?;

        // unlock vested funds first
        let amount_unlocked = vesting_funds.unlock_vested_funds(current_epoch);
        self.locked_funds -= &amount_unlocked;
        if self.locked_funds.is_negative() {
            return Err(format!(
                "negative locked funds {} after unlocking {}",
                self.locked_funds, amount_unlocked
            )
            .into());
        }
        // add locked funds now
        vesting_funds.add_locked_funds(current_epoch, vesting_sum, self.proving_period_start, spec);
        self.locked_funds += vesting_sum;

        // save the updated vesting table state
        self.save_vesting_funds(store, &vesting_funds)?;

        Ok(amount_unlocked)
    }

    /// Draws from vesting table and unlocked funds to repay up to the fee debt.
    /// Returns the amount unlocked from the vesting table and the amount taken from
    /// current balance. If the fee debt exceeds the total amount available for repayment
    /// the fee debt field is updated to track the remaining debt.  Otherwise it is set to zero.
    pub fn repay_partial_debt_in_priority_order<BS: BlockStore>(
        &mut self,
        store: &BS,
        current_epoch: ChainEpoch,
        curr_balance: &TokenAmount,
    ) -> Result<
        (
            TokenAmount, // from vesting
            TokenAmount, // from balance
        ),
        Box<dyn StdError>,
    > {
        let unlocked_balance = self.get_unlocked_balance(curr_balance)?;

        let fee_debt = self.fee_debt.clone();
        let from_vesting = self.unlock_unvested_funds(store, current_epoch, &fee_debt)?;

        // * It may be possible the go implementation catches a potential panic here
        if from_vesting > self.fee_debt {
            return Err("should never unlock more than the debt we need to repay"
                .to_owned()
                .into());
        }
        self.fee_debt -= &from_vesting;

        let from_balance = cmp::min(&unlocked_balance, &self.fee_debt).clone();
        self.fee_debt -= &from_balance;

        Ok((from_vesting, from_balance))
    }

    /// Repays the full miner actor fee debt.  Returns the amount that must be
    /// burnt and an error if there are not sufficient funds to cover repayment.
    /// Miner state repays from unlocked funds and fails if unlocked funds are insufficient to cover fee debt.
    /// FeeDebt will be zero after a successful call.
    pub fn repay_debts(
        &mut self,
        curr_balance: &TokenAmount,
    ) -> Result<TokenAmount, Box<dyn StdError>> {
        let unlocked_balance = self.get_unlocked_balance(curr_balance)?;
        if unlocked_balance < self.fee_debt {
            return Err(actor_error!(
                ErrInsufficientFunds,
                "unlocked balance can not repay fee debt ({} < {})",
                unlocked_balance,
                self.fee_debt
            )
            .into());
        }

        Ok(std::mem::take(&mut self.fee_debt))
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
        if target.is_zero() || self.locked_funds.is_zero() {
            return Ok(TokenAmount::zero());
        }

        let mut vesting_funds = self.load_vesting_funds(store)?;
        let amount_unlocked = vesting_funds.unlock_unvested_funds(current_epoch, target);
        self.locked_funds -= &amount_unlocked;
        if self.locked_funds.is_negative() {
            return Err(format!(
                "negative locked funds {} after unlocking {}",
                self.locked_funds, amount_unlocked
            )
            .into());
        }

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
        if self.locked_funds.is_zero() {
            return Ok(TokenAmount::zero());
        }

        let mut vesting_funds = self.load_vesting_funds(store)?;
        let amount_unlocked = vesting_funds.unlock_vested_funds(current_epoch);
        self.locked_funds -= &amount_unlocked;
        if self.locked_funds.is_negative() {
            return Err(format!(
                "vesting cause locked funds to become negative: {}",
                self.locked_funds,
            )
            .into());
        }

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
    pub fn get_unlocked_balance(&self, actor_balance: &TokenAmount) -> Result<TokenAmount, String> {
        let unlocked_balance =
            actor_balance - &self.locked_funds - &self.pre_commit_deposits - &self.initial_pledge;
        if unlocked_balance.is_negative() {
            return Err(format!("negative unlocked balance {}", unlocked_balance));
        }
        Ok(unlocked_balance)
    }

    /// Unclaimed funds. Actor balance - (locked funds, precommit deposit, ip requirement)
    /// Can go negative if the miner is in IP debt.
    pub fn get_available_balance(
        &self,
        actor_balance: &TokenAmount,
    ) -> Result<TokenAmount, String> {
        // (actor_balance - &self.locked_funds) - &self.pre_commit_deposit
        Ok(self.get_unlocked_balance(actor_balance)? - &self.fee_debt)
    }

    pub fn check_balance_invariants(&self, balance: &TokenAmount) -> Result<(), String> {
        if self.pre_commit_deposits.is_negative() {
            return Err(format!(
                "pre-commit deposit is negative: {}",
                self.pre_commit_deposits
            ));
        }
        if self.locked_funds.is_negative() {
            return Err(format!("locked funds is negative: {}", self.locked_funds));
        }
        if self.initial_pledge.is_negative() {
            return Err(format!(
                "initial pledge is negative: {}",
                self.initial_pledge
            ));
        }
        if self.fee_debt.is_negative() {
            return Err(format!("fee debt is negative: {}", self.fee_debt));
        }

        let min_balance = &self.pre_commit_deposits + &self.locked_funds + &self.initial_pledge;
        if balance < &min_balance {
            return Err(format!("fee debt is negative: {}", self.fee_debt));
        }

        Ok(())
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
        queue.add_to_queue_values(expire_epoch, &[sector_number as usize])?;
        self.pre_committed_sectors_expiry = queue.amt.flush()?;

        Ok(())
    }

    pub fn expire_pre_commits<BS: BlockStore>(
        &mut self,
        store: &BS,
        current_epoch: ChainEpoch,
    ) -> Result<TokenAmount, Box<dyn StdError>> {
        let mut deposit_to_burn = TokenAmount::zero();

        // Expire pre-committed sectors
        let mut expiry_queue = BitFieldQueue::new(
            store,
            &self.pre_committed_sectors_expiry,
            self.quant_spec_every_deadline(),
        )?;

        let (sectors, modified) = expiry_queue.pop_until(current_epoch)?;

        if modified {
            self.pre_committed_sectors_expiry = expiry_queue.amt.flush()?;
        }

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
        if self.pre_commit_deposits.is_negative() {
            return Err(format!(
                "pre-commit expiry caused negative deposits: {}",
                self.pre_commit_deposits
            )
            .into());
        }

        Ok(deposit_to_burn)
    }

    pub fn advance_deadline<BS: BlockStore>(
        &mut self,
        store: &BS,
        current_epoch: ChainEpoch,
    ) -> Result<AdvanceDeadlineResult, Box<dyn StdError>> {
        let mut pledge_delta = TokenAmount::zero();

        let dl_info = self.deadline_info(current_epoch);

        if !dl_info.period_started() {
            return Ok(AdvanceDeadlineResult {
                pledge_delta,
                power_delta: PowerPair::zero(),
                previously_faulty_power: PowerPair::zero(),
                detected_faulty_power: PowerPair::zero(),
                total_faulty_power: PowerPair::zero(),
            });
        }

        self.current_deadline = ((dl_info.index + 1) % WPOST_PERIOD_DEADLINES) as usize;
        if self.current_deadline == 0 {
            self.proving_period_start = dl_info.period_start + WPOST_PROVING_PERIOD;
        }

        let mut deadlines = self.load_deadlines(store)?;

        let mut deadline = deadlines.load_deadline(store, dl_info.index as usize)?;

        let previously_faulty_power = deadline.faulty_power.clone();

        if !deadline.is_live() {
            return Ok(AdvanceDeadlineResult {
                pledge_delta,
                power_delta: PowerPair::zero(),
                previously_faulty_power,
                detected_faulty_power: PowerPair::zero(),
                total_faulty_power: deadline.faulty_power,
            });
        }

        let quant = quant_spec_for_deadline(&dl_info);

        // Detect and penalize missing proofs.
        let fault_expiration = dl_info.last() + FAULT_MAX_AGE;

        let (mut power_delta, detected_faulty_power) =
            deadline.process_deadline_end(store, quant, fault_expiration)?;

        // Capture deadline's faulty power after new faults have been detected, but before it is
        // dropped along with faulty sectors expiring this round.
        let total_faulty_power = deadline.faulty_power.clone();

        // Expire sectors that are due, either for on-time expiration or "early" faulty-for-too-long.
        let expired = deadline.pop_expired_sectors(store, dl_info.last(), quant)?;

        // Release pledge requirements for the sectors expiring on-time.
        // Pledge for the sectors expiring early is retained to support the termination fee that
        // will be assessed when the early termination is processed.
        pledge_delta -= &expired.on_time_pledge;
        self.add_initial_pledge(&expired.on_time_pledge.neg())?;

        // Record reduction in power of the amount of expiring active power.
        // Faulty power has already been lost, so the amount expiring can be excluded from the delta.
        power_delta -= &expired.active_power;

        let no_early_terminations = expired.early_sectors.is_empty();
        if !no_early_terminations {
            self.early_terminations.set(dl_info.index as usize);
        }

        deadlines.update_deadline(store, dl_info.index as usize, &deadline)?;

        self.save_deadlines(store, deadlines)?;

        Ok(AdvanceDeadlineResult {
            pledge_delta,
            power_delta,
            previously_faulty_power,
            detected_faulty_power,
            total_faulty_power,
        })
    }
}

pub struct AdvanceDeadlineResult {
    pub pledge_delta: TokenAmount,
    pub power_delta: PowerPair,
    /// Power that was faulty before this advance (including recovering)
    pub previously_faulty_power: PowerPair,
    /// Power of new faults and failed recoveries
    pub detected_faulty_power: PowerPair,
    /// Total faulty power after detecting faults (before expiring sectors)
    /// Note that failed recovery power is included in both PreviouslyFaultyPower and
    /// DetectedFaultyPower, so TotalFaultyPower is not simply their sum.
    pub total_faulty_power: PowerPair,
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
    pub window_post_proof_type: RegisteredPoStProof,

    /// Amount of space in each sector committed to the network by this miner
    pub sector_size: SectorSize,

    /// The number of sectors in each Window PoSt partition (proof).
    /// This is computed from the proof type and represented here redundantly.
    pub window_post_partition_sectors: u64,

    /// The next epoch this miner is eligible for certain permissioned actor methods
    /// and winning block elections as a result of being reported for a consensus fault.
    pub consensus_fault_elapsed: ChainEpoch,

    /// A proposed new owner account for this miner.
    /// Must be confirmed by a message from the pending address itself.
    pub pending_owner_address: Option<Address>,
}

impl MinerInfo {
    pub fn new(
        owner: Address,
        worker: Address,
        control_addresses: Vec<Address>,
        peer_id: Vec<u8>,
        multi_address: Vec<BytesDe>,
        window_post_proof_type: RegisteredPoStProof,
    ) -> Result<Self, ActorError> {
        let sector_size = window_post_proof_type
            .sector_size()
            .map_err(|e| actor_error!(ErrIllegalArgument, "invalid sector size: {}", e))?;

        let window_post_partition_sectors = window_post_proof_type
            .window_post_partitions_sector()
            .map_err(|e| actor_error!(ErrIllegalArgument, "invalid sector size: {}", e))?;

        Ok(Self {
            owner,
            worker,
            control_addresses,
            pending_worker_key: None,
            peer_id,
            multi_address,
            window_post_proof_type,
            sector_size,
            window_post_partition_sectors,
            consensus_fault_elapsed: EPOCH_UNDEFINED,
            pending_owner_address: None,
        })
    }
}
