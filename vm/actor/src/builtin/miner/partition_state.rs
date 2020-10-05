// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{
    power_for_sectors, validate_partition_contains_sectors, BitFieldQueue, ExpirationQueue,
    ExpirationSet, QuantSpec, SectorOnChainInfo, Sectors, TerminationResult, NO_QUANTIZATION,
};
use crate::{actor_error, ActorDowncast};
use bitfield::BitField;
use cid::Cid;
use clock::ChainEpoch;
use encoding::tuple::*;
use fil_types::{SectorSize, StoragePower};
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser;
use num_traits::Zero;
use std::{error::Error as StdError, ops};
use vm::{ActorError, ExitCode, TokenAmount};

#[derive(Serialize_tuple, Deserialize_tuple, Clone)]
pub struct Partition {
    /// Sector numbers in this partition, including faulty and terminated sectors.
    pub sectors: BitField,
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

    /// Power of not-yet-terminated sectors (incl faulty).
    pub live_power: PowerPair,
    /// Power of currently-faulty sectors. FaultyPower <= LivePower.
    pub faulty_power: PowerPair,
    /// Power of expected-to-recover sectors. RecoveringPower <= FaultyPower.
    pub recovering_power: PowerPair,
}

impl Partition {
    pub fn new(empty_array_cid: Cid) -> Self {
        Self {
            sectors: BitField::new(),
            faults: BitField::new(),
            recoveries: BitField::new(),
            terminated: BitField::new(),
            expirations_epochs: empty_array_cid.clone(),
            early_terminated: empty_array_cid,
            live_power: PowerPair::zero(),
            faulty_power: PowerPair::zero(),
            recovering_power: PowerPair::zero(),
        }
    }

    /// Live sectors are those that are not terminated (but may be faulty).
    pub fn live_sectors(&self) -> BitField {
        &self.sectors - &self.terminated
    }

    /// Active sectors are those that are neither terminated nor faulty, i.e. actively contributing power.
    pub fn active_sectors(&self) -> BitField {
        &self.live_sectors() - &self.faults
    }

    /// Active power is power of non-faulty sectors.
    pub fn active_power(&self) -> PowerPair {
        &self.live_power - &self.faulty_power
    }

    /// AddSectors adds new sectors to the partition.
    /// The sectors are "live", neither faulty, recovering, nor terminated.
    /// Each new sector's expiration is scheduled shortly after its target expiration epoch.
    pub fn add_sectors<BS: BlockStore>(
        &mut self,
        store: &BS,
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
    ) -> Result<PowerPair, Box<dyn StdError>> {
        // Load expiration queue
        let mut queue = ExpirationQueue::new(store, &self.expirations_epochs, quant)
            .map_err(|e| e.downcast_wrap("failed to load partition queue"))?;

        // Reschedule faults
        let power = queue
            .reschedule_as_faults(fault_expiration, sectors, sector_size)
            .map_err(|e| e.downcast_wrap("failed to add faults to partition queue"))?;

        // Save expiration queue
        self.expirations_epochs = queue.amt.flush()?;

        // Update partition metadata
        self.faults |= sector_numbers;

        // The sectors must not have been previously faulty or recovering.
        // No change to recoveries or terminations.

        self.faulty_power += &power;
        // No change to live or recovering power.

        Ok(power)
    }

    /// Declares a set of sectors faulty. Already faulty sectors are ignored,
    /// terminated sectors are skipped, and recovering sectors are reverted to
    /// faulty.
    ///
    /// - New faults are added to the Faults bitfield and the FaultyPower is increased.
    /// - The sectors' expirations are rescheduled to the fault expiration epoch, as "early" (if not expiring earlier).
    ///
    /// Returns the power of the now-faulty sectors.
    pub fn declare_faults<BS: BlockStore>(
        &mut self,
        store: &BS,
        sectors: &Sectors<'_, BS>,
        sector_numbers: &BitField,
        fault_expiration_epoch: ChainEpoch,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> Result<(BitField, PowerPair), Box<dyn StdError>> {
        validate_partition_contains_sectors(&self, sector_numbers)
            .map_err(|e| actor_error!(ErrIllegalArgument; "failed fault declaration: {}", e))?;

        // Split declarations into declarations of new faults, and retraction of declared recoveries.
        let retracted_recoveries = &self.recoveries & sector_numbers;
        let mut new_faults = sector_numbers - &retracted_recoveries;

        // Ignore any terminated sectors and previously declared or detected faults
        new_faults -= &self.terminated;
        new_faults -= &self.faults;

        // Add new faults to state.
        let mut new_faulty_power = PowerPair::zero();
        let new_fault_sectors = sectors
            .load_sector(&new_faults)
            .map_err(|e| e.wrap("failed to load fault sectors"))?;

        if !new_fault_sectors.is_empty() {
            new_faulty_power = self
                .add_faults(
                    store,
                    &new_faults,
                    &new_fault_sectors,
                    fault_expiration_epoch,
                    sector_size,
                    quant,
                )
                .map_err(|e| e.downcast_wrap("failed to add faults"))?;
        }

        Ok((new_faults, new_faulty_power))
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
        self.faulty_power -= &power;
        self.recovering_power -= &power;

        Ok(power)
    }

    /// Declares sectors as recovering. Non-faulty and already recovering sectors will be skipped.
    pub fn declare_faults_recovered<BS: BlockStore>(
        &mut self,
        sectors: &Sectors<'_, BS>,
        sector_size: SectorSize,
        sector_numbers: &BitField,
    ) -> Result<(), ActorError> {
        // Check that the declared sectors are actually assigned to the partition.
        validate_partition_contains_sectors(self, sector_numbers)
            .map_err(|e| actor_error!(ErrIllegalArgument; "failed fault declaration: {}", e))?;

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

        // No change to faults, or terminations.
        // No change to faulty power.

        Ok(())
    }

    /// Removes sectors from recoveries and recovering power. Assumes sectors are currently faulty and recovering.
    pub fn remove_recoveries(&mut self, sector_numbers: &BitField, power: &PowerPair) {
        if sector_numbers.is_empty() {
            return;
        }

        self.recoveries -= sector_numbers;
        self.recovering_power -= power;

        // 	// No change to faults, or terminations.
        // 	// No change to faulty power.
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
        sector_numbers: &BitField,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> Result<BitField, Box<dyn StdError>> {
        // Ensure these sectors actually belong to this partition.
        let present = sector_numbers & &self.sectors;

        // Filter out terminated sectors.
        let live = &present - &self.terminated;

        // Filter out faulty sectors.
        let active = &live - &self.faults;

        let sector_infos = sectors.load_sector(&active)?;
        let mut expirations = ExpirationQueue::new(store, &self.expirations_epochs, quant)
            .map_err(|e| e.downcast_wrap("failed to load sector expirations"))?;
        expirations.reschedule_expirations(new_expiration, &sector_infos, sector_size)?;
        self.expirations_epochs = expirations.amt.flush()?;

        Ok(active)
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
        sector_numbers: &BitField,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> Result<ExpirationSet, Box<dyn StdError>> {
        let live_sectors = self.live_sectors();
        if live_sectors.contains_all(sector_numbers) {
            return Err(actor_error!(ErrIllegalArgument; "can only terminate live sectors").into());
        }

        let sector_infos = sectors.load_sector(sector_numbers)?;
        let mut expirations = ExpirationQueue::new(store, &self.expirations_epochs, quant)
            .map_err(|e| e.downcast_wrap("failed to load sector expirations"))?;
        let (removed, removed_recovering) = expirations
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

        // Update partition metadata.
        self.faults -= &removed_sectors;
        self.recoveries -= &removed_sectors;
        self.terminated |= &removed_sectors;
        self.live_power -= &removed.active_power;
        self.live_power -= &removed.faulty_power;
        self.faulty_power -= &removed.faulty_power;
        self.recovering_power -= &removed_recovering;

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
    ) -> Result<(PowerPair, PowerPair), Box<dyn StdError>> {
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
        let new_fault_power = &self.live_power - &self.faulty_power;
        let failed_recovery_power = self.recovering_power.clone();

        // Update partition metadata
        let all_faults = self.live_sectors();
        self.faults = all_faults;
        self.recoveries = BitField::new();
        self.faulty_power = self.live_power.clone();
        self.recovering_power = PowerPair::zero();

        Ok((new_fault_power, failed_recovery_power))
    }

    pub fn pop_early_terminations<BS: BlockStore>(
        &mut self,
        store: &BS,
        max_sectors: u64,
    ) -> Result<(TerminationResult, /* has more */ bool), Box<dyn StdError>> {
        // Load early terminations.
        let mut early_terminated_queue =
            BitFieldQueue::new(store, &self.early_terminated, NO_QUANTIZATION)?;

        let mut processed = Vec::<u64>::new();
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
            .batch_delete(processed)
            .map_err(|e| {
                e.downcast_wrap("failed to remove entries from early terminations queue")
            })?;

        if let Some((remaining_sectors, remaining_epoch)) = remaining.take() {
            early_terminated_queue
                .amt
                .set(remaining_epoch as u64, remaining_sectors)
                .map_err(|e| {
                    e.downcast_wrap("failed to update remaining entry early terminations queue")
                })?;
        }

        // Save early terminations.
        self.early_terminated = early_terminated_queue
            .amt
            .flush()
            .map_err(|e| e.downcast_wrap("failed to store early terminations queue"))?;

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
        skipped: &BitField,
    ) -> Result<(PowerPair, PowerPair), ActorError> {
        if skipped.is_empty() {
            return Ok((PowerPair::zero(), PowerPair::zero()));
        }

        // Check that the declared sectors are actually in the partition.
        if !self.sectors.contains_all(skipped) {
            return Err(
                actor_error!(ErrIllegalArgument; "skipped faults contains sectors outside partition"),
            );
        }

        // Find all skipped faults that have been labeled recovered
        let retracted_recoveries = &self.recoveries & skipped;
        let retracted_recovery_sectors = sectors
            .load_sector(&retracted_recoveries)
            .map_err(|e| e.wrap("failed to load sectors"))?;
        let retracted_recovery_power = power_for_sectors(sector_size, &retracted_recovery_sectors);

        // Ignore skipped faults that are already faults or terminated.
        let new_faults = &(skipped - &self.terminated) - &self.faults;
        let new_fault_sectors = sectors
            .load_sector(&new_faults)
            .map_err(|e| e.wrap("failed to load sectors"))?;

        // Record new faults
        let new_fault_power = self
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

        Ok((new_fault_power, retracted_recovery_power))
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
