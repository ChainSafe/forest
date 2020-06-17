// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::deadlines::{compute_proving_period_deadline, DeadlineInfo};
use super::policy::*;
use super::types::*;
use crate::{power, u64_key, BytesKey, HAMT_BIT_WIDTH};
use address::Address;
use bitfield::BitField;
use cid::{multihash::Blake2b256, Cid};
use clock::ChainEpoch;
use encoding::{serde_bytes, tuple::*, Cbor};
use fil_types::{RegisteredSealProof, SectorInfo, SectorNumber, SectorSize};
use ipld_amt::{Amt, Error as AmtError};
use ipld_blockstore::BlockStore;
use ipld_hamt::{Error as HamtError, Hamt};
use num_bigint::biguint_ser::{self, BigUintDe};
use num_bigint::BigUint;
use num_traits::ToPrimitive;
use num_traits::Zero;
use vm::TokenAmount;

// Balance of Miner Actor should be greater than or equal to
// the sum of PreCommitDeposits and LockedFunds.
// Excess balance as computed by st.GetAvailableBalance will be
// withdrawable or usable for pre-commit deposit or pledge lock-up.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct State {
    /// Contains static info about this miner
    // TODO revisit as will likely change to Cid in future
    pub info: MinerInfo,

    /// Total funds locked as pre_commit_deposit
    #[serde(with = "biguint_ser")]
    pub pre_commit_deposit: TokenAmount,
    /// Total unvested funds locked as pledge collateral
    #[serde(with = "biguint_ser")]
    pub locked_funds: TokenAmount,
    /// Array, AMT[ChainEpoch]TokenAmount
    vesting_funds: Cid,

    /// Sectors that have been pre-committed but not yet proven.
    /// Map, HAMT<SectorNumber, SectorPreCommitOnChainInfo>
    pub pre_committed_sectors: Cid,

    /// Sectors this miner has committed
    /// Array, AMT<SectorOnChainInfo>
    pub sectors: Cid,

    /// The first epoch in this miner's current proving period. This is the first epoch in which a PoSt for a
    /// partition at the miner's first deadline may arrive. Alternatively, it is after the last epoch at which
    /// a PoSt for the previous window is valid.
    /// Always greater than zero, his may be greater than the current epoch for genesis miners in the first
    /// WPoStProvingPeriod epochs of the chain; the epochs before the first proving period starts are exempt from Window
    /// PoSt requirements.
    /// Updated at the end of every period by a power actor cron event.
    pub proving_period_start: ChainEpoch,

    /// Sector numbers prove-committed since period start, to be added to Deadlines at next proving period boundary.
    pub new_sectors: BitField,

    /// Sector numbers indexed by expiry epoch (which are on proving period boundaries).
    /// Invariant: Keys(Sectors) == union(SectorExpirations.Values())
    /// Array, AMT[ChainEpoch]Bitfield
    pub sector_expirations: Cid,

    /// The sector numbers due for PoSt at each deadline in the current proving period, frozen at period start.
    /// New sectors are added and expired ones removed at proving period boundary.
    /// Faults are not subtracted from this in state, but on the fly.
    pub deadlines: Cid,

    /// All currently known faulty sectors, mutated eagerly.
    /// These sectors are exempt from inclusion in PoSt.
    pub faults: BitField,

    /// Faulty sector numbers indexed by the start epoch of the proving period in which detected.
    /// Used to track fault durations for eventual sector termination.
    /// At most 14 entries, b/c sectors faulty longer expire.
    /// Invariant: Faults == union(FaultEpochs.Values())
    /// AMT[ChainEpoch]Bitfield
    pub fault_epochs: Cid,

    /// Faulty sectors that will recover when next included in a valid PoSt.
    /// Invariant: Recoveries ⊆ Faults.
    pub recoveries: BitField,

    /// Records successful PoSt submission in the current proving period by partition number.
    /// The presence of a partition number indicates on-time PoSt received.
    pub post_submissions: BitField,

    /// The index of the next deadline for which faults should been detected and processed (after it's closed).
    /// The proving period cron handler will always reset this to 0, for the subsequent period.
    /// Eager fault detection processing on fault/recovery declarations or PoSt may set a smaller number,
    /// indicating partial progress, from which subsequent processing should continue.
    /// In the range [0, WPoStProvingPeriodDeadlines).
    pub next_deadline_to_process_faults: usize,
}

impl Cbor for State {}

impl State {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        empty_arr: Cid,
        empty_map: Cid,
        empty_deadlines: Cid,
        owner: Address,
        worker: Address,
        peer_id: Vec<u8>,
        multi_address: Vec<u8>,
        seal_proof_type: RegisteredSealProof,
        period_start: ChainEpoch,
    ) -> Result<Self, String> {
        let sector_size = seal_proof_type.sector_size()?;
        let window_post_partition_sectors = seal_proof_type.window_post_partitions_sector()?;
        Ok(Self {
            info: MinerInfo {
                owner,
                worker,
                pending_worker_key: None,
                peer_id,
                multi_address,
                seal_proof_type,
                sector_size,
                window_post_partition_sectors,
            },
            pre_commit_deposit: TokenAmount::default(),
            locked_funds: TokenAmount::default(),
            vesting_funds: empty_arr.clone(),
            pre_committed_sectors: empty_map,
            sectors: empty_arr.clone(),
            proving_period_start: period_start,
            new_sectors: BitField::default(),
            sector_expirations: empty_arr.clone(),
            deadlines: empty_deadlines,
            faults: BitField::default(),
            fault_epochs: empty_arr,
            recoveries: BitField::default(),
            post_submissions: BitField::default(),
            next_deadline_to_process_faults: 0,
        })
    }
    /// Returns worker address
    pub fn get_worker(&self) -> &Address {
        &self.info.worker
    }
    /// Returns sector size
    pub fn get_sector_size(&self) -> &SectorSize {
        &self.info.sector_size
    }
    /// Returns deadline calculations for the current proving period.
    pub fn deadline_info(&self, current_epoch: ChainEpoch) -> DeadlineInfo {
        compute_proving_period_deadline(self.proving_period_start, current_epoch)
    }
    pub fn sector_count<BS: BlockStore>(&self, store: &BS) -> Result<u64, AmtError> {
        let arr = Amt::<SectorOnChainInfo, _>::load(&self.sectors, store)?;

        Ok(arr.count())
    }
    pub fn get_max_allowed_faults<BS: BlockStore>(&self, store: &BS) -> Result<u64, AmtError> {
        let sector_count = self.sector_count(store)?;
        Ok(2 * sector_count)
    }
    pub fn put_precommitted_sector<BS: BlockStore>(
        &mut self,
        store: &BS,
        info: SectorPreCommitOnChainInfo,
    ) -> Result<(), HamtError> {
        let mut precommitted =
            Hamt::load_with_bit_width(&self.pre_committed_sectors, store, HAMT_BIT_WIDTH)?;
        precommitted.set(u64_key(info.info.sector_number), info)?;

        self.pre_committed_sectors = precommitted.flush()?;
        Ok(())
    }
    pub fn get_precommitted_sector<BS: BlockStore>(
        &self,
        store: &BS,
        sector_num: SectorNumber,
    ) -> Result<Option<SectorPreCommitOnChainInfo>, HamtError> {
        let precommitted = Hamt::<BytesKey, _>::load_with_bit_width(
            &self.pre_committed_sectors,
            store,
            HAMT_BIT_WIDTH,
        )?;
        precommitted.get(&u64_key(sector_num))
    }
    pub fn delete_precommitted_sector<BS: BlockStore>(
        &mut self,
        store: &BS,
        sector_num: SectorNumber,
    ) -> Result<(), HamtError> {
        let mut precommitted = Hamt::<BytesKey, _>::load_with_bit_width(
            &self.pre_committed_sectors,
            store,
            HAMT_BIT_WIDTH,
        )?;
        precommitted.delete(&u64_key(sector_num))?;

        self.pre_committed_sectors = precommitted.flush()?;
        Ok(())
    }
    pub fn has_sector_number<BS: BlockStore>(
        &self,
        store: &BS,
        sector_num: SectorNumber,
    ) -> Result<bool, AmtError> {
        let sectors = Amt::<SectorOnChainInfo, _>::load(&self.sectors, store)?;
        Ok(sectors.get(sector_num)?.is_some())
    }
    pub fn put_sector<BS: BlockStore>(
        &mut self,
        store: &BS,
        sector: SectorOnChainInfo,
    ) -> Result<(), AmtError> {
        let mut sectors = Amt::load(&self.sectors, store)?;
        sectors.set(sector.info.sector_number, sector)?;

        self.sectors = sectors.flush()?;
        Ok(())
    }
    pub fn get_sector<BS: BlockStore>(
        &self,
        store: &BS,
        sector_num: SectorNumber,
    ) -> Result<Option<SectorOnChainInfo>, AmtError> {
        let sectors = Amt::<SectorOnChainInfo, _>::load(&self.sectors, store)?;
        sectors.get(sector_num)
    }
    pub fn delete_sector<BS: BlockStore>(
        &mut self,
        store: &BS,
        sector_nos: &mut BitField,
    ) -> Result<(), AmtError> {
        let mut sectors = Amt::<SectorOnChainInfo, _>::load(&self.sectors, store)?;

        sector_nos
            .for_each(|sector_num| {
                sectors.delete(sector_num)?;
                Ok(())
            })
            .map_err(|e| AmtError::Other(format!("could not delete sector number: {}", e)))?;

        self.sectors = sectors.flush()?;
        Ok(())
    }
    pub fn for_each_sector<BS: BlockStore, F>(&self, store: &BS, mut f: F) -> Result<(), String>
    where
        F: FnMut(&SectorOnChainInfo) -> Result<(), String>,
    {
        let sectors = Amt::<SectorOnChainInfo, _>::load(&self.sectors, store)?;
        sectors.for_each(|_, v| f(&v))
    }
    /// Adds some sector numbers to the new sectors bitfield.
    pub fn add_new_sectors(&mut self, sector_nos: &[SectorNumber]) -> Result<(), String> {
        let mut ns = BitField::new();
        for &sector in sector_nos {
            ns.set(sector)
        }
        self.new_sectors.merge_assign(&ns)?;

        let count = self.new_sectors.count()?;
        if count > NEW_SECTORS_PER_PERIOD_MAX {
            return Err(format!(
                "too many new sectors {}, max {}",
                count, NEW_SECTORS_PER_PERIOD_MAX
            ));
        }

        Ok(())
    }
    /// Removes some sector numbers from the new sectors bitfield, if present.
    pub fn remove_new_sectors(&mut self, sector_nos: &BitField) -> Result<(), String> {
        self.new_sectors.subtract_assign(&sector_nos)?;
        Ok(())
    }
    /// Gets the sector numbers expiring at some epoch.
    pub fn get_sector_expirations<BS: BlockStore>(
        &self,
        store: &BS,
        expiry: ChainEpoch,
    ) -> Result<BitField, String> {
        let sectors = Amt::<BitField, _>::load(&self.sector_expirations, store)?;
        Ok(sectors.get(expiry)?.ok_or("unable to find sector")?)
    }
    /// Iterates sector expiration groups in order.
    /// Note that the sectors bitfield provided to the callback is not safe to store.
    pub fn for_each_sector_expiration<BS: BlockStore, F>(
        &self,
        store: &BS,
        mut f: F,
    ) -> Result<(), String>
    where
        F: FnMut(ChainEpoch, &BitField) -> Result<(), String>,
    {
        let sector_arr = Amt::<BitField, _>::load(&self.sector_expirations, store)?;
        sector_arr.for_each(|i, v| f(i, v))
    }
    /// Adds some sector numbers to the set expiring at an epoch.
    /// The sector numbers are given as uint64s to avoid pointless conversions.
    pub fn add_sector_expirations<BS: BlockStore>(
        &mut self,
        store: &BS,
        expiry: ChainEpoch,
        sectors: &[u64],
    ) -> Result<(), String> {
        let mut sector_arr = Amt::<BitField, _>::load(&self.sector_expirations, store)?;
        let mut bf: BitField = sector_arr.get(expiry)?.ok_or("unable to find sector")?;
        bf.merge_assign(&BitField::new_from_set(sectors))?;
        let count = bf.count()?;
        if count > SECTORS_MAX {
            return Err(format!(
                "too many sectors at expiration {}, {}, max {}",
                expiry, count, SECTORS_MAX
            ));
        }

        sector_arr.set(expiry, bf)?;

        self.sector_expirations = sector_arr.flush()?;
        Ok(())
    }
    /// Removes some sector numbers from the set expiring at an epoch.
    pub fn remove_sector_expirations<BS: BlockStore>(
        &mut self,
        store: &BS,
        expiry: ChainEpoch,
        sectors: &[u64],
    ) -> Result<(), String> {
        let mut sector_arr = Amt::<BitField, _>::load(&self.sector_expirations, store)?;

        let bf: BitField = sector_arr.get(expiry)?.ok_or("unable to find sector")?;
        bf.clone()
            .subtract_assign(&BitField::new_from_set(sectors))?;

        sector_arr.set(expiry, bf)?;

        self.sector_expirations = sector_arr.flush()?;

        Ok(())
    }
    /// Removes all sector numbers from the set expiring some epochs.
    pub fn clear_sector_expirations<BS: BlockStore>(
        &mut self,
        store: &BS,
        expirations: &[ChainEpoch],
    ) -> Result<(), String> {
        let mut sector_arr = Amt::<BitField, _>::load(&self.sector_expirations, store)?;

        for &exp in expirations {
            sector_arr.delete(exp)?;
        }

        self.sector_expirations = sector_arr.flush()?;

        Ok(())
    }
    /// Adds sectors numbers to faults and fault epochs.
    pub fn add_faults<BS: BlockStore>(
        &mut self,
        store: &BS,
        sector_nos: &mut BitField,
        fault_epoch: ChainEpoch,
    ) -> Result<(), String> {
        if sector_nos.is_empty()? {
            return Err(format!("sectors are empty: {:?}", sector_nos));
        }

        self.faults.merge_assign(sector_nos)?;

        let count = self.faults.count()?;
        if count > SECTORS_MAX {
            return Err(format!("too many faults {}, max {}", count, SECTORS_MAX));
        }

        let mut epoch_fault_arr = Amt::<BitField, _>::load(&self.fault_epochs, store)?;
        let mut bf: BitField = epoch_fault_arr
            .get(fault_epoch)?
            .ok_or("unable to find sector")?;

        bf.merge_assign(sector_nos)?;

        epoch_fault_arr.set(fault_epoch, bf)?;

        self.fault_epochs = epoch_fault_arr.flush()?;

        Ok(())
    }
    /// Removes sector numbers from faults and fault epochs, if present.
    pub fn remove_faults<BS: BlockStore>(
        &mut self,
        store: &BS,
        sector_nos: &mut BitField,
    ) -> Result<(), String> {
        if sector_nos.is_empty()? {
            return Err(format!("sectors are empty: {:?}", sector_nos));
        }

        self.faults.subtract_assign(sector_nos)?;

        let mut sector_arr = Amt::<BitField, _>::load(&self.fault_epochs, store)?;

        let mut changed: Vec<(u64, BitField)> = Vec::new();

        sector_arr.for_each(|i, bf1: &BitField| {
            let c1 = bf1.clone().count()?;

            let mut bf2 = bf1.clone().subtract(sector_nos)?;

            let c2 = bf2.count()?;

            if c1 != c2 {
                changed.push((i, bf2));
            }

            Ok(())
        })?;

        for (k, v) in changed.into_iter() {
            sector_arr.set(k, v)?;
        }

        self.fault_epochs = sector_arr.flush()?;

        Ok(())
    }
    /// Iterates faults by declaration epoch, in order.
    pub fn for_each_fault_epoch<BS: BlockStore, F>(
        &self,
        store: &BS,
        mut f: F,
    ) -> Result<(), String>
    where
        F: FnMut(ChainEpoch, &BitField) -> Result<(), String>,
    {
        let sector_arr = Amt::<BitField, _>::load(&self.fault_epochs, store)?;

        sector_arr.for_each(|i, v| f(i, v))
    }
    pub fn clear_fault_epochs<BS: BlockStore>(
        &mut self,
        store: &BS,
        epochs: &[ChainEpoch],
    ) -> Result<(), String> {
        let mut epoch_fault_arr = Amt::<BitField, _>::load(&self.fault_epochs, store)?;

        for &exp in epochs {
            epoch_fault_arr.delete(exp)?;
        }

        self.fault_epochs = epoch_fault_arr.flush()?;

        Ok(())
    }
    /// Adds sectors to recoveries.
    pub fn add_recoveries(&mut self, sector_nos: &mut BitField) -> Result<(), String> {
        if sector_nos.is_empty()? {
            return Err(format!("sectors are empty: {:?}", sector_nos));
        }

        self.recoveries.clone().merge_assign(sector_nos)?;

        let count = self.recoveries.count()?;
        if count > SECTORS_MAX {
            return Err(format!(
                "too many recoveries {}, max {}",
                count, SECTORS_MAX
            ));
        }

        Ok(())
    }
    /// Removes sectors from recoveries, if present.
    pub fn remove_recoveries(&mut self, sector_nos: &mut BitField) -> Result<(), String> {
        if sector_nos.is_empty()? {
            return Err(format!("sectors are empty: {:?}", sector_nos));
        }
        self.recoveries.subtract_assign(sector_nos)?;

        Ok(())
    }
    /// Loads sector info for a sequence of sectors.
    pub fn load_sector_infos<BS: BlockStore>(
        &self,
        store: &BS,
        sectors: &mut BitField,
    ) -> Result<Vec<SectorOnChainInfo>, String> {
        let mut sector_infos: Vec<SectorOnChainInfo> = Vec::new();
        sectors.for_each(|i| {
            let key: SectorNumber = i;
            let sector_on_chain = self
                .get_sector(store, key)?
                .ok_or(format!("sector not found: {}", i))?;
            sector_infos.push(sector_on_chain);
            Ok(())
        })?;

        Ok(sector_infos)
    }

    /// Loads info for a set of sectors to be proven.
    /// If any of the sectors are declared faulty and not to be recovered, info for the first non-faulty sector is substituted instead.
    /// If any of the sectors are declared recovered, they are returned from this method.
    pub fn load_sector_infos_for_proof<BS: BlockStore>(
        &mut self,
        store: &BS,
        mut proven_sectors: BitField,
    ) -> Result<(Vec<SectorOnChainInfo>, BitField), String> {
        // Extract a fault set relevant to the sectors being submitted, for expansion into a map.
        let declared_faults = self.faults.clone().intersect(&proven_sectors)?;

        let recoveries = self.recoveries.clone().intersect(&declared_faults)?;

        let mut expected_faults = declared_faults.subtract(&recoveries)?;

        let mut non_faults = expected_faults.clone().subtract(&proven_sectors)?;

        if non_faults.is_empty()? {
            return Err(format!(
                "failed to check if bitfield was empty: {:?}",
                non_faults
            ));
        }

        // Select a non-faulty sector as a substitute for faulty ones.
        let good_sector_no = non_faults.first()?;

        // load sector infos
        let sector_infos = self.load_sector_infos_with_fault_mask(
            store,
            &mut proven_sectors,
            &mut expected_faults,
            good_sector_no,
        )?;

        Ok((sector_infos, recoveries))
    }
    /// Loads sector info for a sequence of sectors, substituting info for a stand-in sector for any that are faulty.
    fn load_sector_infos_with_fault_mask<BS: BlockStore>(
        &self,
        store: &BS,
        sectors: &mut BitField,
        faults: &mut BitField,
        fault_stand_in: SectorNumber,
    ) -> Result<Vec<SectorOnChainInfo>, String> {
        let sector_on_chain = self
            .get_sector(store, fault_stand_in)?
            .ok_or(format!("can't find stand-in sector {}", fault_stand_in))?;

        // Expand faults into a map for quick lookups.
        // The faults bitfield should already be a subset of the sectors bitfield.
        let fault_max = sectors.count()?;
        let fault_set = faults.all_set(fault_max)?;

        // Load the sector infos, masking out fault sectors with a good one.
        let mut sector_infos: Vec<SectorOnChainInfo> = Vec::new();
        sectors.for_each(|i| {
            let mut sector = sector_on_chain.clone();
            let _faulty = fault_set.get(&i).ok_or_else(|| {
                let new_sector_on_chain = self
                    .get_sector(store, fault_stand_in)
                    .unwrap()
                    .ok_or(format!("unable to find sector: {}", i))
                    .unwrap();
                sector = new_sector_on_chain;
            });

            sector_infos.push(sector);
            Ok(())
        })?;
        Ok(sector_infos)
    }
    /// Adds partition numbers to the set of PoSt submissions
    pub fn add_post_submissions(&mut self, partition_nos: BitField) -> Result<(), String> {
        self.post_submissions.merge_assign(&partition_nos)?;
        Ok(())
    }
    /// Removes all PoSt submissions
    pub fn clear_post_submissions(&mut self) -> Result<(), String> {
        self.post_submissions = BitField::new();
        Ok(())
    }
    pub fn load_deadlines<BS: BlockStore>(&self, store: &BS) -> Result<Deadlines, String> {
        if let Some(deadlines) = store
            .get::<Deadlines>(&self.deadlines)
            .map_err(|e| e.to_string())?
        {
            Ok(deadlines)
        } else {
            Err(format!("failed to load deadlines: {}", self.deadlines))
        }
    }
    pub fn save_deadlines<BS: BlockStore>(
        &mut self,
        store: &BS,
        deadlines: Deadlines,
    ) -> Result<(), String> {
        let c = store
            .put(&deadlines, Blake2b256)
            .map_err(|e| e.to_string())?;
        self.deadlines = c;
        Ok(())
    }

    //
    // Funds and vesting
    //

    pub fn add_pre_commit_deposit(&mut self, amount: &TokenAmount) {
        self.pre_commit_deposit += amount
    }

    pub fn subtract_pre_commit_deposit(&mut self, amount: &TokenAmount) {
        self.pre_commit_deposit -= amount
    }

    pub fn add_locked_funds<BS: BlockStore>(
        &mut self,
        store: &BS,
        current_epoch: ChainEpoch,
        vesting_sum: &TokenAmount,
        spec: VestSpec,
    ) -> Result<(), AmtError> {
        let mut vesting_funds: Amt<u64, _> = Amt::load(&self.vesting_funds, store)?;

        // Nothing unlocks here, this is just the start of the clock
        let vest_begin = current_epoch + spec.initial_delay;
        let vest_period = BigUint::from(spec.vest_period);
        let mut e = vest_begin + spec.step_duration;
        let mut vested_so_far = BigUint::zero();

        while &vested_so_far < vesting_sum {
            let vest_epoch = quantize_up(e, spec.quantization);
            let elapsed = vest_epoch - vest_begin;

            let target_vest = if elapsed < spec.vest_period {
                // Linear vesting, PARAM_FINISH
                (vesting_sum * elapsed) / &vest_period
            } else {
                vesting_sum.clone()
            };

            let vest_this_time = &target_vest - vested_so_far;
            vested_so_far = target_vest;

            // Load existing entry, else set a new one
            if let Some(locked_fund_entry) = vesting_funds.get(vest_epoch)? {
                let mut locked_funds = BigUint::from(locked_fund_entry);
                locked_funds += vest_this_time;

                let num = ToPrimitive::to_u64(&locked_funds)
                    .ok_or("unable to convert to u64")
                    .unwrap();
                vesting_funds.set(vest_epoch, num)?;
            }
            e += spec.step_duration;
        }
        self.vesting_funds = vesting_funds.flush()?;
        self.locked_funds += vesting_sum;

        Ok(())
    }

    /// Unlocks an amount of funds that have *not yet vested*, if possible.
    /// The soonest-vesting entries are unlocked first.
    /// Returns the amount actually unlocked.
    pub fn unlock_unvested_funds<BS: BlockStore>(
        &mut self,
        store: &BS,
        current_epoch: ChainEpoch,
        target: TokenAmount,
    ) -> Result<TokenAmount, String> {
        let mut vesting_funds: Amt<BigUintDe, _> = Amt::load(&self.vesting_funds, store)?;

        let mut amount_unlocked = TokenAmount::default();
        let mut to_del: Vec<u64> = Vec::new();

        let mut set: Vec<(u64, BigUintDe)> = Vec::new();
        vesting_funds.for_each(|k, v| {
            if amount_unlocked > target {
                if k >= current_epoch {
                    let BigUintDe(mut locked_entry) = v.clone();
                    let unlock_amount =
                        std::cmp::min(target.clone() - &amount_unlocked, locked_entry.clone());
                    amount_unlocked += &unlock_amount;
                    locked_entry -= &unlock_amount;

                    if locked_entry.is_zero() {
                        to_del.push(k);
                    } else {
                        set.push((k, BigUintDe(locked_entry)));
                    }
                }
            } else {
                // stop iterating
                return Err("finished".to_string());
            }
            Ok(())
        })?;

        for (k, v) in set {
            vesting_funds.set(k, v)?;
        }

        delete_many(&mut vesting_funds, &to_del)?;

        self.locked_funds -= &amount_unlocked;
        self.vesting_funds = vesting_funds.flush()?;

        Ok(amount_unlocked)
    }

    /// Unlocks all vesting funds that have vested before the provided epoch.
    /// Returns the amount unlocked.
    pub fn unlock_vested_funds<BS: BlockStore>(
        &mut self,
        store: &BS,
        current_epoch: ChainEpoch,
    ) -> Result<TokenAmount, String> {
        let mut vesting_funds: Amt<BigUintDe, _> = Amt::load(&self.vesting_funds, store)?;

        let mut amount_unlocked = TokenAmount::default();
        let mut to_del: Vec<u64> = Vec::new();

        vesting_funds.for_each(|k, v| {
            if k < current_epoch {
                let BigUintDe(locked_entry) = v;
                amount_unlocked += locked_entry;
                to_del.push(k);
            } else {
                // stop iterating
                return Err("finished".to_string());
            }
            Ok(())
        })?;

        delete_many(&mut vesting_funds, &to_del)?;

        self.locked_funds -= &amount_unlocked;
        self.vesting_funds = vesting_funds.flush()?;

        Ok(amount_unlocked)
    }

    /// CheckVestedFunds returns the amount of vested funds that have vested before the provided epoch.
    pub fn check_vested_funds<BS: BlockStore>(
        &self,
        store: &BS,
        current_epoch: ChainEpoch,
    ) -> Result<TokenAmount, String> {
        let vesting_funds: Amt<BigUintDe, _> = Amt::load(&self.vesting_funds, store)?;

        let mut amount_unlocked = TokenAmount::default();
        vesting_funds.for_each(|k, v| {
            if k < current_epoch {
                let BigUintDe(locked_entry) = v.clone();
                amount_unlocked += locked_entry;
            } else {
                // stop iterating
                return Err("finished".to_string());
            }
            Ok(())
        })?;

        Ok(amount_unlocked)
    }

    pub fn get_available_balance(&self, actor_balance: &TokenAmount) -> TokenAmount {
        (actor_balance - &self.locked_funds) - &self.pre_commit_deposit
    }

    pub fn assert_balance_invariants(&self, balance: &TokenAmount) {
        assert!(balance > &(&self.pre_commit_deposit + &self.locked_funds));
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

    /// Optional worker key to update at an epoch
    pub pending_worker_key: Option<WorkerKeyChange>,

    /// Libp2p identity that should be used when connecting to this miner
    #[serde(with = "serde_bytes")]
    pub peer_id: Vec<u8>,
    /// Slice of byte arrays representing Libp2p multi-addresses used for establishing a connection with this miner.
    #[serde(with = "serde_bytes")]
    pub multi_address: Vec<u8>,

    /// The proof type used by this miner for sealing sectors.
    pub seal_proof_type: RegisteredSealProof,

    /// Amount of space in each sector committed to the network by this miner
    pub sector_size: SectorSize,

    /// The number of sectors in each Window PoSt partition (proof).
    /// This is computed from the proof type and represented here redundantly.
    pub window_post_partition_sectors: u64,
}

impl SectorOnChainInfo {
    pub fn new(
        info: SectorPreCommitInfo,
        activation_epoch: ChainEpoch,
        deal_weight: BigUint,
        verified_deal_weight: BigUint,
    ) -> Self {
        Self {
            info,
            activation_epoch,
            deal_weight,
            verified_deal_weight,
        }
    }
    pub fn to_sector_info(&self) -> SectorInfo {
        SectorInfo {
            proof: self.info.registered_proof,
            sector_number: self.info.sector_number,
            sealed_cid: self.info.sealed_cid.clone(),
        }
    }
}

pub fn to_storage_weight_desc(
    sector_size: SectorSize,
    sector_info: &SectorOnChainInfo,
) -> power::SectorStorageWeightDesc {
    power::SectorStorageWeightDesc {
        sector_size,
        deal_weight: sector_info.deal_weight.clone(),
        verified_deal_weight: sector_info.verified_deal_weight.clone(),
        duration: sector_info.info.expiration - sector_info.activation_epoch,
    }
}

//
// PoSt Deadlines and partitions
//
#[derive(Debug, Clone, Serialize_tuple, Deserialize_tuple)]
pub struct Deadlines {
    /// A bitfield of sector numbers due at each deadline.
    /// The sectors for each deadline are logically grouped into sequential partitions for proving.
    pub due: Vec<BitField>,
}

impl Default for Deadlines {
    fn default() -> Self {
        Self::new()
    }
}

impl Deadlines {
    pub fn new() -> Self {
        Self {
            due: vec![BitField::new(); WPOST_PERIOD_DEADLINES],
        }
    }

    /// Adds sector numbers to a deadline.
    /// The sector numbers are given as uint64 to avoid pointless conversions for bitfield use.
    pub fn add_to_deadline(&mut self, deadline: usize, new_sectors: &[u64]) -> Result<(), String> {
        let ns = BitField::new_from_set(new_sectors);
        let sec = self
            .due
            .get_mut(deadline)
            .ok_or(format!("unable to find deadline: {}", deadline))?;
        sec.merge_assign(&ns)?;
        Ok(())
    }
    /// Removes sector numbers from all deadlines.
    pub fn remove_from_all_deadlines(&mut self, sector_nos: &mut BitField) -> Result<(), String> {
        for d in self.due.iter_mut() {
            d.subtract_assign(&sector_nos)?;
        }

        Ok(())
    }
}

//
// Misc helpers
//

fn delete_many<BS: BlockStore>(amt: &mut Amt<BigUintDe, BS>, keys: &[u64]) -> Result<(), AmtError> {
    for &i in keys {
        amt.delete(i)?;
    }
    Ok(())
}

/// Rounds e to the nearest exact multiple of the quantization unit, rounding up.
/// Precondition: unit >= 0 else behaviour is undefined
fn quantize_up(e: ChainEpoch, unit: ChainEpoch) -> ChainEpoch {
    let rem = e % unit;
    if rem == 0 {
        return e;
    }
    e - rem + unit
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
            pending_worker_key: None,
            peer_id: PeerId::random().into_bytes(),
            multi_address: PeerId::random().into_bytes(),
            sector_size: SectorSize::_2KiB,
            seal_proof_type: RegisteredSealProof::from(1),
            window_post_partition_sectors: 0,
        };
        let bz = to_vec(&info).unwrap();
        assert_eq!(from_slice::<MinerInfo>(&bz).unwrap(), info);
    }
}
