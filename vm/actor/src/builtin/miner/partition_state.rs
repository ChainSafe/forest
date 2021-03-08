// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{
    power_for_sectors, select_sectors, validate_partition_contains_sectors, BitFieldQueue,
    ExpirationQueue, ExpirationSet, SectorOnChainInfo, Sectors, TerminationResult,
};
use crate::{actor_error, ActorDowncast};
use bitfield::{BitField, UnvalidatedBitField, Validate};
use cid::Cid;
use clock::ChainEpoch;
use encoding::tuple::*;
use fil_types::{
    deadlines::{QuantSpec, NO_QUANTIZATION},
    SectorSize, StoragePower,
};
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser;
use num_traits::{Signed, Zero};
use std::{
    error::Error as StdError,
    ops::{self, Neg},
};
use vm::{ActorError, ExitCode, TokenAmount};

// Bitwidth of AMTs determined empirically from mutation patterns and projections of mainnet data.
const PARTITION_EXPIRATION_AMT_BITWIDTH: usize = 4;
const PARTITION_EARLY_TERMINATION_ARRAY_AMT_BITWIDTH: usize = 3;

#[derive(Serialize_tuple, Deserialize_tuple, Clone)]
pub struct Partition {
    /// Sector numbers in this partition, including faulty, unproven and terminated sectors.
    pub sectors: BitField,
    /// Unproven sectors in this partition. This bitfield will be cleared on
    /// a successful window post (or at the end of the partition's next
    /// deadline). At that time, any still unproven sectors will be added to
    /// the faulty sector bitfield.
    pub unproven: BitField,
    /// Subset of sectors detected/declared faulty and not yet recovered (excl. from PoSt).
    /// Faults ∩ Terminated = ∅
    pub faults: BitField,
    /// Subset of faulty sectors expected to recover on next PoSt
    /// Recoveries ∩ Terminated = ∅
    pub recoveries: BitField,
    /// Subset of sectors terminated but not yet removed from partition (excl. from PoSt)
    pub terminated: BitField,
    /// Maps epochs sectors that expire in or before that epoch.
    /// An expiration may be an "on-time" scheduled expiration, or early "faulty" expiration.
    /// Keys are quantized to last-in-deadline epochs.
    pub expirations_epochs: Cid, // AMT[ChainEpoch]ExpirationSet
    /// Subset of terminated that were before their committed expiration epoch, by termination epoch.
    /// Termination fees have not yet been calculated or paid and associated deals have not yet been
    /// canceled but effective power has already been adjusted.
    /// Not quantized.
    pub early_terminated: Cid, // AMT[ChainEpoch]BitField

    /// Power of not-yet-terminated sectors (incl faulty & unproven).
    pub live_power: PowerPair,
    /// Power of yet-to-be-proved sectors (never faulty).
    pub unproven_power: PowerPair,
    /// Power of currently-faulty sectors. FaultyPower <= LivePower.
    pub faulty_power: PowerPair,
    /// Power of expected-to-recover sectors. RecoveringPower <= FaultyPower.
    pub recovering_power: PowerPair,
}

impl Partition {
    pub fn new<BS: BlockStore>(store: &BS) -> Result<Self, Box<dyn StdError>> {
        let empty_expiration_array =
            Amt::<Cid, BS>::new_with_bit_width(store, PARTITION_EXPIRATION_AMT_BITWIDTH).flush()?;
        let empty_early_termination_array = Amt::<Cid, BS>::new_with_bit_width(
            store,
            PARTITION_EARLY_TERMINATION_ARRAY_AMT_BITWIDTH,
        )
        .flush()?;

        Ok(Self {
            sectors: BitField::new(),
            unproven: BitField::new(),
            faults: BitField::new(),
            recoveries: BitField::new(),
            terminated: BitField::new(),
            expirations_epochs: empty_expiration_array,
            early_terminated: empty_early_termination_array,
            live_power: PowerPair::zero(),
            unproven_power: PowerPair::zero(),
            faulty_power: PowerPair::zero(),
            recovering_power: PowerPair::zero(),
        })
    }

    /// Live sectors are those that are not terminated (but may be faulty).
    pub fn live_sectors(&self) -> BitField {
        &self.sectors - &self.terminated
    }

    /// Active sectors are those that are neither terminated nor faulty nor unproven, i.e. actively contributing power.
    pub fn active_sectors(&self) -> BitField {
        let non_faulty = &self.live_sectors() - &self.faults;
        &non_faulty - &self.unproven
    }

    /// Active power is power of non-faulty sectors.
    pub fn active_power(&self) -> PowerPair {
        &(&self.live_power - &self.faulty_power) - &self.unproven_power
    }

    /// AddSectors adds new sectors to the partition.
    /// The sectors are "live", neither faulty, recovering, nor terminated.
    /// Each new sector's expiration is scheduled shortly after its target expiration epoch.
    pub fn add_sectors<BS: BlockStore>(
        &mut self,
        store: &BS,
        proven: bool,
        sectors: &[SectorOnChainInfo],
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> Result<PowerPair, Box<dyn StdError>> {
        let mut expirations = ExpirationQueue::new(store, &self.expirations_epochs, quant)
            .map_err(|e| e.downcast_wrap("failed to load sector expirations"))?;

        let (sector_numbers, power, _) = expirations
            .add_active_sectors(sectors, sector_size)
            .map_err(|e| e.downcast_wrap("failed to record new sector expirations"))?;

        self.expirations_epochs = expirations
            .amt
            .flush()
            .map_err(|e| e.downcast_wrap("failed to store sector expirations"))?;

        if self.sectors.contains_any(&sector_numbers) {
            return Err("not all added sectors are new".into());
        }

        // Update other metadata using the calculated totals.
        self.sectors |= &sector_numbers;
        self.live_power += &power;

        if !proven {
            self.unproven_power += &power;
            self.unproven |= &sector_numbers;
        }

        // check invariants
        self.validate_state()?;

        // No change to faults, recoveries, or terminations.
        // No change to faulty or recovering power.
        Ok(power)
    }

    /// marks a set of sectors faulty
    pub fn add_faults<BS: BlockStore>(
        &mut self,
        store: &BS,
        sector_numbers: &BitField,
        sectors: &[SectorOnChainInfo],
        fault_expiration: ChainEpoch,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> Result<(PowerPair, PowerPair), Box<dyn StdError>> {
        // Load expiration queue
        let mut queue = ExpirationQueue::new(store, &self.expirations_epochs, quant)
            .map_err(|e| e.downcast_wrap("failed to load partition queue"))?;

        // Reschedule faults
        let new_faulty_power = queue
            .reschedule_as_faults(fault_expiration, sectors, sector_size)
            .map_err(|e| e.downcast_wrap("failed to add faults to partition queue"))?;

        // Save expiration queue
        self.expirations_epochs = queue.amt.flush()?;

        // Update partition metadata
        self.faults |= sector_numbers;

        // The sectors must not have been previously faulty or recovering.
        // No change to recoveries or terminations.
        self.faulty_power += &new_faulty_power;

        // Once marked faulty, sectors are moved out of the unproven set.
        let unproven = sector_numbers & &self.unproven;

        self.unproven -= &unproven;

        let mut power_delta = new_faulty_power.clone().neg();

        let unproven_infos = select_sectors(sectors, &unproven)
            .map_err(|e| e.downcast_wrap("failed to select unproven sectors"))?;
        if !unproven_infos.is_empty() {
            let lost_unproven_power = power_for_sectors(sector_size, &unproven_infos);
            self.unproven_power -= &lost_unproven_power;
            power_delta += &lost_unproven_power;
        }

        // check invariants
        self.validate_state()?;

        Ok((power_delta, new_faulty_power))
    }

    /// Declares a set of sectors faulty. Already faulty sectors are ignored,
    /// terminated sectors are skipped, and recovering sectors are reverted to
    /// faulty.
    ///
    /// - New faults are added to the Faults bitfield and the FaultyPower is increased.
    /// - The sectors' expirations are rescheduled to the fault expiration epoch, as "early" (if not expiring earlier).
    ///
    /// Returns the power of the now-faulty sectors.
    pub fn record_faults<BS: BlockStore>(
        &mut self,
        store: &BS,
        sectors: &Sectors<'_, BS>,
        sector_numbers: &mut UnvalidatedBitField,
        fault_expiration_epoch: ChainEpoch,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> Result<(BitField, PowerPair, PowerPair), Box<dyn StdError>> {
        validate_partition_contains_sectors(&self, sector_numbers)
            .map_err(|e| actor_error!(ErrIllegalArgument; "failed fault declaration: {}", e))?;

        let sector_numbers = sector_numbers
            .validate()
            .map_err(|e| format!("failed to intersect sectors with recoveries: {}", e))?;

        // Split declarations into declarations of new faults, and retraction of declared recoveries.
        let retracted_recoveries = &self.recoveries & sector_numbers;
        let mut new_faults = sector_numbers - &retracted_recoveries;

        // Ignore any terminated sectors and previously declared or detected faults
        new_faults -= &self.terminated;
        new_faults -= &self.faults;

        // Add new faults to state.
        let new_fault_sectors = sectors
            .load_sector(&new_faults)
            .map_err(|e| e.wrap("failed to load fault sectors"))?;

        let (power_delta, new_faulty_power) = if !new_fault_sectors.is_empty() {
            self.add_faults(
                store,
                &new_faults,
                &new_fault_sectors,
                fault_expiration_epoch,
                sector_size,
                quant,
            )
            .map_err(|e| e.downcast_wrap("failed to add faults"))?
        } else {
            Default::default()
        };

        // check invariants
        self.validate_state()?;

        Ok((new_faults, power_delta, new_faulty_power))
    }

    /// Removes sector numbers from faults and thus from recoveries.
    /// The sectors are removed from the Faults and Recovering bitfields, and FaultyPower and RecoveringPower reduced.
    /// The sectors are re-scheduled for expiration shortly after their target expiration epoch.
    /// Returns the power of the now-recovered sectors.
    pub fn recover_faults<BS: BlockStore>(
        &mut self,
        store: &BS,
        sectors: &Sectors<'_, BS>,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> Result<PowerPair, Box<dyn StdError>> {
        // Process recoveries, assuming the proof will be successful.
        // This similarly updates state.
        let recovered_sectors = sectors
            .load_sector(&self.recoveries)
            .map_err(|e| e.wrap("failed to load recovered sectors"))?;

        // Load expiration queue
        let mut queue = ExpirationQueue::new(store, &self.expirations_epochs, quant)
            .map_err(|e| format!("failed to load partition queue: {:?}", e))?;

        // Reschedule recovered
        let power = queue
            .reschedule_recovered(recovered_sectors, sector_size)
            .map_err(|e| e.downcast_wrap("failed to reschedule faults in partition queue"))?;

        // Save expiration queue
        self.expirations_epochs = queue.amt.flush()?;

        // Update partition metadata
        self.faults -= &self.recoveries;
        self.recoveries = BitField::new();

        // No change to live power.
        // No change to unproven sectors.
        self.faulty_power -= &power;
        self.recovering_power -= &power;

        // check invariants
        self.validate_state()?;

        Ok(power)
    }

    /// Activates unproven sectors, returning the activated power.
    pub fn activate_unproven(&mut self) -> PowerPair {
        self.unproven = BitField::default();
        std::mem::take(&mut self.unproven_power)
    }

    /// Declares sectors as recovering. Non-faulty and already recovering sectors will be skipped.
    pub fn declare_faults_recovered<BS: BlockStore>(
        &mut self,
        sectors: &Sectors<'_, BS>,
        sector_size: SectorSize,
        sector_numbers: &mut UnvalidatedBitField,
    ) -> Result<(), Box<dyn StdError>> {
        // Check that the declared sectors are actually assigned to the partition.
        validate_partition_contains_sectors(self, sector_numbers)
            .map_err(|e| actor_error!(ErrIllegalArgument; "failed fault declaration: {}", e))?;

        let sector_numbers = sector_numbers
            .validate()
            .map_err(|e| format!("failed to validate recoveries: {}", e))?;

        // Ignore sectors not faulty or already declared recovered
        let mut recoveries = sector_numbers & &self.faults;
        recoveries -= &self.recoveries;

        // Record the new recoveries for processing at Window PoSt or deadline cron.
        let recovery_sectors = sectors
            .load_sector(&recoveries)
            .map_err(|e| e.wrap("failed to load recovery sectors"))?;

        self.recoveries |= &recoveries;

        let power = power_for_sectors(sector_size, &recovery_sectors);
        self.recovering_power += &power;

        // check invariants
        self.validate_state()?;

        // No change to faults, or terminations.
        // No change to faulty power.
        // No change to unproven power/sectors.
        Ok(())
    }

    /// Removes sectors from recoveries and recovering power. Assumes sectors are currently faulty and recovering.
    pub fn remove_recoveries(&mut self, sector_numbers: &BitField, power: &PowerPair) {
        if sector_numbers.is_empty() {
            return;
        }

        self.recoveries -= sector_numbers;
        self.recovering_power -= power;

        // No change to faults, or terminations.
        // No change to faulty power.
        // No change to unproven power.
    }

    /// RescheduleExpirations moves expiring sectors to the target expiration,
    /// skipping any sectors it can't find.
    ///
    /// The power of the rescheduled sectors is assumed to have not changed since
    /// initial scheduling.
    ///
    /// Note: see the docs on State.RescheduleSectorExpirations for details on why we
    /// skip sectors/partitions we can't find.
    pub fn reschedule_expirations<BS: BlockStore>(
        &mut self,
        store: &BS,
        sectors: &Sectors<'_, BS>,
        new_expiration: ChainEpoch,
        sector_numbers: &mut UnvalidatedBitField,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> Result<Vec<SectorOnChainInfo>, Box<dyn StdError>> {
        let sector_numbers = sector_numbers.validate()?;

        // Ensure these sectors actually belong to this partition.
        let present = &*sector_numbers & &self.sectors;

        // Filter out terminated sectors.
        let live = &present - &self.terminated;

        // Filter out faulty sectors.
        let active = &live - &self.faults;

        let sector_infos = sectors.load_sector(&active)?;
        let mut expirations = ExpirationQueue::new(store, &self.expirations_epochs, quant)
            .map_err(|e| e.downcast_wrap("failed to load sector expirations"))?;
        expirations.reschedule_expirations(new_expiration, &sector_infos, sector_size)?;
        self.expirations_epochs = expirations.amt.flush()?;

        // check invariants
        self.validate_state()?;

        Ok(sector_infos)
    }

    /// Replaces a number of "old" sectors with new ones.
    /// The old sectors must not be faulty or terminated.
    /// If the same sector is both removed and added, this permits rescheduling *with a change in power*,
    /// unlike RescheduleExpirations.
    /// Returns the delta to power and pledge requirement.
    pub fn replace_sectors<BS: BlockStore>(
        &mut self,
        store: &BS,
        old_sectors: &[SectorOnChainInfo],
        new_sectors: &[SectorOnChainInfo],
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> Result<(PowerPair, TokenAmount), Box<dyn StdError>> {
        let mut expirations = ExpirationQueue::new(store, &self.expirations_epochs, quant)
            .map_err(|e| e.downcast_wrap("failed to load sector expirations"))?;

        let (old_sector_numbers, new_sector_numbers, power_delta, pledge_delta) = expirations
            .replace_sectors(old_sectors, new_sectors, sector_size)
            .map_err(|e| e.downcast_wrap("failed to replace sector expirations"))?;

        self.expirations_epochs = expirations
            .amt
            .flush()
            .map_err(|e| e.downcast_wrap("failed to save sector expirations"))?;

        // Check the sectors being removed are active (alive, not faulty).
        let active = self.active_sectors();
        let all_active = active.contains_all(&old_sector_numbers);

        if !all_active {
            return Err(format!(
                "refusing to replace inactive sectors in {:?} (active: {:?})",
                old_sector_numbers, active
            )
            .into());
        }

        // Update partition metadata.
        self.sectors -= &old_sector_numbers;
        self.sectors |= &new_sector_numbers;
        self.live_power += &power_delta;

        // check invariants
        self.validate_state()?;

        // No change to faults, recoveries, or terminations.
        // No change to faulty or recovering power.
        Ok((power_delta, pledge_delta))
    }

    /// Record the epoch of any sectors expiring early, for termination fee calculation later.
    pub fn record_early_termination<BS: BlockStore>(
        &mut self,
        store: &BS,
        epoch: ChainEpoch,
        sectors: &BitField,
    ) -> Result<(), Box<dyn StdError>> {
        let mut early_termination_queue =
            BitFieldQueue::new(store, &self.early_terminated, NO_QUANTIZATION)
                .map_err(|e| e.downcast_wrap("failed to load early termination queue"))?;

        early_termination_queue
            .add_to_queue(epoch, sectors)
            .map_err(|e| e.downcast_wrap("failed to add to early termination queue"))?;

        self.early_terminated = early_termination_queue
            .amt
            .flush()
            .map_err(|e| e.downcast_wrap("failed to save early termination queue"))?;

        Ok(())
    }

    /// Marks a collection of sectors as terminated.
    /// The sectors are removed from Faults and Recoveries.
    /// The epoch of termination is recorded for future termination fee calculation.
    pub fn terminate_sectors<BS: BlockStore>(
        &mut self,
        store: &BS,
        sectors: &Sectors<'_, BS>,
        epoch: ChainEpoch,
        sector_numbers: &mut UnvalidatedBitField,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> Result<ExpirationSet, Box<dyn StdError>> {
        let live_sectors = self.live_sectors();
        let sector_numbers = sector_numbers.validate().map_err(|e| {
            actor_error!(
                ErrIllegalArgument,
                "failed to validate terminating sectors: {}",
                e
            )
        })?;

        if !live_sectors.contains_all(sector_numbers) {
            return Err(actor_error!(ErrIllegalArgument, "can only terminate live sectors").into());
        }

        let sector_infos = sectors.load_sector(sector_numbers)?;
        let mut expirations = ExpirationQueue::new(store, &self.expirations_epochs, quant)
            .map_err(|e| e.downcast_wrap("failed to load sector expirations"))?;
        let (mut removed, removed_recovering) = expirations
            .remove_sectors(&sector_infos, &self.faults, &self.recoveries, sector_size)
            .map_err(|e| e.downcast_wrap("failed to remove sector expirations"))?;

        self.expirations_epochs = expirations
            .amt
            .flush()
            .map_err(|e| e.downcast_wrap("failed to save sector expirations"))?;

        let removed_sectors = &removed.on_time_sectors | &removed.early_sectors;

        // Record early termination.
        self.record_early_termination(store, epoch, &removed_sectors)
            .map_err(|e| e.downcast_wrap("failed to record early sector termination"))?;

        let unproven_nos = &removed_sectors & &self.unproven;

        // Update partition metadata.
        self.faults -= &removed_sectors;
        self.recoveries -= &removed_sectors;
        self.terminated |= &removed_sectors;
        self.live_power -= &removed.active_power;
        self.live_power -= &removed.faulty_power;
        self.faulty_power -= &removed.faulty_power;
        self.recovering_power -= &removed_recovering;
        self.unproven -= &unproven_nos;

        let unproven_infos = select_sectors(&sector_infos, &unproven_nos)?;
        let removed_unproven_power = power_for_sectors(sector_size, &unproven_infos);
        self.unproven_power -= &removed_unproven_power;
        removed.active_power -= &removed_unproven_power;

        // check invariants
        self.validate_state()?;

        Ok(removed)
    }

    /// PopExpiredSectors traverses the expiration queue up to and including some epoch, and marks all expiring
    /// sectors as terminated.
    /// Returns the expired sector aggregates.
    pub fn pop_expired_sectors<BS: BlockStore>(
        &mut self,
        store: &BS,
        until: ChainEpoch,
        quant: QuantSpec,
    ) -> Result<ExpirationSet, Box<dyn StdError>> {
        // This is a sanity check to make sure we handle proofs _before_
        // handling sector expirations.
        if !self.unproven.is_empty() {
            return Err("Cannot pop expired sectors from a partition with unproven sectors".into());
        }

        let mut expirations = ExpirationQueue::new(store, &self.expirations_epochs, quant)
            .map_err(|e| e.downcast_wrap("failed to load expiration queue"))?;
        let popped = expirations.pop_until(until).map_err(|e| {
            e.downcast_wrap(format!("failed to pop expiration queue until {}", until))
        })?;
        self.expirations_epochs = expirations.amt.flush()?;

        let expired_sectors = &popped.on_time_sectors | &popped.early_sectors;

        // There shouldn't be any recovering sectors or power if this is invoked at deadline end.
        // Either the partition was PoSted and the recovering became recovered, or the partition was not PoSted
        // and all recoveries retracted.
        // No recoveries may be posted until the deadline is closed.
        if !self.recoveries.is_empty() {
            return Err("unexpected recoveries while processing expirations".into());
        }
        if !self.recovering_power.is_zero() {
            return Err("unexpected recovering power while processing expirations".into());
        }

        // Nothing expiring now should have already terminated.
        if self.terminated.contains_any(&expired_sectors) {
            return Err("expiring sectors already terminated".into());
        }

        // Mark the sectors as terminated and subtract sector power.
        self.terminated |= &expired_sectors;
        self.faults -= &expired_sectors;
        self.live_power -= &(&popped.active_power + &popped.faulty_power);
        self.faulty_power -= &popped.faulty_power;

        // Record the epoch of any sectors expiring early, for termination fee calculation later.
        self.record_early_termination(store, until, &popped.early_sectors)
            .map_err(|e| e.downcast_wrap("failed to record early terminations"))?;

        // check invariants
        self.validate_state()?;

        Ok(popped)
    }

    /// Marks all non-faulty sectors in the partition as faulty and clears recoveries, updating power memos appropriately.
    /// All sectors' expirations are rescheduled to the fault expiration, as "early" (if not expiring earlier)
    /// Returns the power of the newly faulty and failed recovery sectors.
    pub fn record_missed_post<BS: BlockStore>(
        &mut self,
        store: &BS,
        fault_expiration: ChainEpoch,
        quant: QuantSpec,
    ) -> Result<(PowerPair, PowerPair, PowerPair), Box<dyn StdError>> {
        // Collapse tail of queue into the last entry, and mark all power faulty.
        // Load expiration queue
        let mut queue = ExpirationQueue::new(store, &self.expirations_epochs, quant)
            .map_err(|e| e.downcast_wrap("failed to load partition queue"))?;

        queue
            .reschedule_all_as_faults(fault_expiration)
            .map_err(|e| e.downcast_wrap("failed to reschedule all as faults"))?;

        // Save expiration queue
        self.expirations_epochs = queue.amt.flush()?;

        // Compute faulty power for penalization. New faulty power is the total power minus already faulty.
        let new_faulty_power = &self.live_power - &self.faulty_power;
        // Penalized power is the newly faulty power, plus the failed recovery power.
        let penalized_power = &self.recovering_power + &new_faulty_power;

        // The power delta is -(newFaultyPower-unproven), because unproven power
        // was never activated in the first place.
        let power_delta = &self.unproven_power - &new_faulty_power;

        // Update partition metadata
        let all_faults = self.live_sectors();
        self.faults = all_faults;
        self.recoveries = BitField::new();
        self.unproven = BitField::new();
        self.faulty_power = self.live_power.clone();
        self.recovering_power = PowerPair::zero();
        self.unproven_power = PowerPair::zero();

        // check invariants
        self.validate_state()?;

        Ok((power_delta, penalized_power, new_faulty_power))
    }

    pub fn pop_early_terminations<BS: BlockStore>(
        &mut self,
        store: &BS,
        max_sectors: u64,
    ) -> Result<(TerminationResult, /* has more */ bool), Box<dyn StdError>> {
        // Load early terminations.
        let mut early_terminated_queue =
            BitFieldQueue::new(store, &self.early_terminated, NO_QUANTIZATION)?;

        let mut processed = Vec::<usize>::new();
        let mut remaining: Option<(BitField, ChainEpoch)> = None;
        let mut result = TerminationResult::new();
        result.partitions_processed = 1;

        early_terminated_queue
            .amt
            .for_each_while(|i, sectors| {
                let epoch = i as ChainEpoch;
                let count = sectors.len() as u64;
                let limit = max_sectors - result.sectors_processed;

                let to_process = if limit < count {
                    let to_process = sectors
                        .slice(0, limit as usize)
                        .map_err(|e| format!("failed to slice early terminations: {}", e))?;
                    let rest = sectors - &to_process;
                    remaining = Some((rest, epoch));
                    result.sectors_processed += limit;
                    to_process
                } else {
                    processed.push(i);
                    result.sectors_processed += count;
                    sectors.clone()
                };

                result.sectors.insert(epoch, to_process);

                let keep_going = result.sectors_processed < max_sectors;
                Ok(keep_going)
            })
            .map_err(|e| e.downcast_wrap("failed to walk early terminations queue"))?;

        // Update early terminations
        early_terminated_queue
            .amt
            .batch_delete(processed, true)
            .map_err(|e| {
                e.downcast_wrap("failed to remove entries from early terminations queue")
            })?;

        if let Some((remaining_sectors, remaining_epoch)) = remaining.take() {
            early_terminated_queue
                .amt
                .set(remaining_epoch as usize, remaining_sectors)
                .map_err(|e| {
                    e.downcast_wrap("failed to update remaining entry early terminations queue")
                })?;
        }

        // Save early terminations.
        self.early_terminated = early_terminated_queue
            .amt
            .flush()
            .map_err(|e| e.downcast_wrap("failed to store early terminations queue"))?;

        // check invariants
        self.validate_state()?;

        let has_more = early_terminated_queue.amt.count() > 0;
        Ok((result, has_more))
    }

    /// Discovers how skipped faults declared during post intersect with existing faults and recoveries, records the
    /// new faults in state.
    /// Returns the amount of power newly faulty, or declared recovered but faulty again.
    ///
    /// - Skipped faults that are not in the provided partition triggers an error.
    /// - Skipped faults that are already declared (but not delcared recovered) are ignored.
    pub fn record_skipped_faults<BS: BlockStore>(
        &mut self,
        store: &BS,
        sectors: &Sectors<'_, BS>,
        sector_size: SectorSize,
        quant: QuantSpec,
        fault_expiration: ChainEpoch,
        skipped: &mut UnvalidatedBitField,
    ) -> Result<(PowerPair, PowerPair, PowerPair, bool), Box<dyn StdError>> {
        let skipped = skipped.validate().map_err(|e| {
            actor_error!(
                ErrIllegalArgument,
                "failed to validate skipped sectors: {}",
                e
            )
        })?;

        if skipped.is_empty() {
            return Ok((
                PowerPair::zero(),
                PowerPair::zero(),
                PowerPair::zero(),
                false,
            ));
        }

        // Check that the declared sectors are actually in the partition.
        if !self.sectors.contains_all(&skipped) {
            return Err(actor_error!(
                ErrIllegalArgument,
                "skipped faults contains sectors outside partition"
            )
            .into());
        }

        // Find all skipped faults that have been labeled recovered
        let retracted_recoveries = &self.recoveries & skipped;
        let retracted_recovery_sectors = sectors
            .load_sector(&retracted_recoveries)
            .map_err(|e| e.wrap("failed to load sectors"))?;
        let retracted_recovery_power = power_for_sectors(sector_size, &retracted_recovery_sectors);

        // Ignore skipped faults that are already faults or terminated.
        let new_faults = &(&*skipped - &self.terminated) - &self.faults;
        let new_fault_sectors = sectors
            .load_sector(&new_faults)
            .map_err(|e| e.wrap("failed to load sectors"))?;

        // Record new faults
        let (power_delta, new_fault_power) = self
            .add_faults(
                store,
                &new_faults,
                &new_fault_sectors,
                fault_expiration,
                sector_size,
                quant,
            )
            .map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to add skipped faults")
            })?;

        // Remove faulty recoveries
        self.remove_recoveries(&retracted_recoveries, &retracted_recovery_power);

        // check invariants
        self.validate_state()?;

        Ok((
            power_delta,
            new_fault_power,
            retracted_recovery_power,
            !new_fault_sectors.is_empty(),
        ))
    }

    /// Test invariants about the partition power are valid.
    pub fn validate_power_state(&self) -> Result<(), &'static str> {
        if self.live_power.raw.is_negative() || self.live_power.qa.is_negative() {
            return Err("Partition left with negative live power");
        }
        if self.unproven_power.raw.is_negative() || self.unproven_power.qa.is_negative() {
            return Err("Partition left with negative unproven power");
        }
        if self.faulty_power.raw.is_negative() || self.faulty_power.qa.is_negative() {
            return Err("Partition left with negative faulty power");
        }
        if self.recovering_power.raw.is_negative() || self.recovering_power.qa.is_negative() {
            return Err("Partition left with negative recovering power");
        }
        if self.unproven_power.raw > self.live_power.raw {
            return Err("Partition left with invalid unproven power");
        }
        if self.faulty_power.raw > self.live_power.raw {
            return Err("Partition left with invalid faulty power");
        }
        // The first half of this conditional shouldn't matter, keeping for readability
        if self.recovering_power.raw > self.live_power.raw
            || self.recovering_power.raw > self.faulty_power.raw
        {
            return Err("Partition left with invalid recovering power");
        }

        Ok(())
    }

    pub fn validate_bf_state(&self) -> Result<(), &'static str> {
        let mut merge = &self.unproven | &self.faults;

        // Unproven or faulty sectors should not be in terminated
        if self.terminated.contains_any(&merge) {
            return Err("Partition left with terminated sectors in multiple states");
        }

        merge |= &self.terminated;

        // All merged sectors should exist in partition sectors
        if !self.sectors.contains_all(&merge) {
            return Err("Partition left with invalid sector state");
        }

        // All recoveries should exist in partition faults
        if !self.faults.contains_all(&self.recoveries) {
            return Err("Partition left with invalid recovery state");
        }

        Ok(())
    }

    pub fn validate_state(&self) -> Result<(), String> {
        self.validate_power_state()?;
        self.validate_bf_state()?;
        Ok(())
    }
}

#[derive(Serialize_tuple, Deserialize_tuple, Eq, PartialEq, Clone, Debug, Default)]
// Value type for a pair of raw and QA power.
pub struct PowerPair {
    #[serde(with = "bigint_ser")]
    pub raw: StoragePower,
    #[serde(with = "bigint_ser")]
    pub qa: StoragePower,
}

impl PowerPair {
    pub fn zero() -> Self {
        Default::default()
    }

    pub fn is_zero(&self) -> bool {
        self.raw.is_zero() && self.qa.is_zero()
    }
}

impl ops::Add for &PowerPair {
    type Output = PowerPair;

    fn add(self, rhs: Self) -> Self::Output {
        PowerPair {
            raw: &self.raw + &rhs.raw,
            qa: &self.qa + &rhs.qa,
        }
    }
}

impl ops::AddAssign<&Self> for PowerPair {
    fn add_assign(&mut self, rhs: &Self) {
        *self = &*self + rhs;
    }
}

impl ops::Sub for &PowerPair {
    type Output = PowerPair;

    fn sub(self, rhs: Self) -> Self::Output {
        PowerPair {
            raw: &self.raw - &rhs.raw,
            qa: &self.qa - &rhs.qa,
        }
    }
}

impl ops::SubAssign<&Self> for PowerPair {
    fn sub_assign(&mut self, rhs: &Self) {
        *self = &*self - rhs;
    }
}

impl ops::Neg for PowerPair {
    type Output = PowerPair;

    fn neg(self) -> Self::Output {
        PowerPair {
            raw: -self.raw,
            qa: -self.qa,
        }
    }
}

impl ops::Neg for &PowerPair {
    type Output = PowerPair;

    fn neg(self) -> Self::Output {
        -self.clone()
    }
}
