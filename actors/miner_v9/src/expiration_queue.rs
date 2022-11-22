// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::{BTreeMap, BTreeSet};
use std::convert::TryInto;

use anyhow::{anyhow, Context};
use cid::Cid;
use fil_actors_runtime_v9::runtime::Policy;
use fil_actors_runtime_v9::{ActorDowncast, Array};
use fvm_ipld_amt::{Error as AmtError, ValueMut};
use fvm_ipld_bitfield::BitField;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::tuple::*;

use fvm_shared::clock::{ChainEpoch, QuantSpec};
use fvm_shared::econ::TokenAmount;
use fvm_shared::sector::{SectorNumber, SectorSize};
use num_traits::{Signed, Zero};

use super::{power_for_sector, PowerPair, SectorOnChainInfo};

/// An internal limit on the cardinality of a bitfield in a queue entry.
/// This must be at least large enough to support the maximum number of sectors in a partition.
/// It would be a bit better to derive this number from an enumeration over all partition sizes.
const ENTRY_SECTORS_MAX: u64 = 10_000;

/// ExpirationSet is a collection of sector numbers that are expiring, either due to
/// expected "on-time" expiration at the end of their life, or unexpected "early" termination
/// due to being faulty for too long consecutively.
/// Note that there is not a direct correspondence between on-time sectors and active power;
/// a sector may be faulty but expiring on-time if it faults just prior to expected termination.
/// Early sectors are always faulty, and active power always represents on-time sectors.
#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug, Default)]
pub struct ExpirationSet {
    /// Sectors expiring "on time" at the end of their committed life
    pub on_time_sectors: BitField,
    /// Sectors expiring "early" due to being faulty for too long
    pub early_sectors: BitField,
    /// Pledge total for the on-time sectors
    pub on_time_pledge: TokenAmount,
    /// Power that is currently active (not faulty)
    pub active_power: PowerPair,
    /// Power that is currently faulty
    pub faulty_power: PowerPair,
}

impl ExpirationSet {
    pub fn empty() -> Self {
        Default::default()
    }

    /// Adds sectors and power to the expiration set in place.
    pub fn add(
        &mut self,
        on_time_sectors: &BitField,
        early_sectors: &BitField,
        on_time_pledge: &TokenAmount,
        active_power: &PowerPair,
        faulty_power: &PowerPair,
    ) -> anyhow::Result<()> {
        self.on_time_sectors |= on_time_sectors;
        self.early_sectors |= early_sectors;
        self.on_time_pledge += on_time_pledge;
        self.active_power += active_power;
        self.faulty_power += faulty_power;

        self.validate_state()?;
        Ok(())
    }

    /// Removes sectors and power from the expiration set in place.
    pub fn remove(
        &mut self,
        on_time_sectors: &BitField,
        early_sectors: &BitField,
        on_time_pledge: &TokenAmount,
        active_power: &PowerPair,
        faulty_power: &PowerPair,
    ) -> anyhow::Result<()> {
        // Check for sector intersection. This could be cheaper with a combined intersection/difference method used below.
        if !self.on_time_sectors.contains_all(on_time_sectors) {
            return Err(anyhow!(
                "removing on-time sectors {:?} not contained in {:?}",
                on_time_sectors,
                self.on_time_sectors
            ));
        }
        if !self.early_sectors.contains_all(early_sectors) {
            return Err(anyhow!(
                "removing early sectors {:?} not contained in {:?}",
                early_sectors,
                self.early_sectors
            ));
        }

        self.on_time_sectors -= on_time_sectors;
        self.early_sectors -= early_sectors;
        self.on_time_pledge -= on_time_pledge;
        self.active_power -= active_power;
        self.faulty_power -= faulty_power;

        // Check underflow.
        if self.on_time_pledge.is_negative() {
            return Err(anyhow!("expiration set pledge underflow: {:?}", self));
        }
        if self.active_power.qa.is_negative() || self.faulty_power.qa.is_negative() {
            return Err(anyhow!("expiration set power underflow: {:?}", self));
        }
        self.validate_state()?;
        Ok(())
    }

    /// A set is empty if it has no sectors.
    /// The power and pledge are not checked, but are expected to be zero.
    pub fn is_empty(&self) -> bool {
        self.on_time_sectors.is_empty() && self.early_sectors.is_empty()
    }

    /// Counts all sectors in the expiration set.
    pub fn len(&self) -> u64 {
        self.on_time_sectors.len() + self.early_sectors.len()
    }

    /// validates a set of assertions that must hold for expiration sets
    pub fn validate_state(&self) -> anyhow::Result<()> {
        if self.on_time_pledge.is_negative() {
            return Err(anyhow!("ExpirationSet left with negative pledge"));
        }

        if self.active_power.raw.is_negative() {
            return Err(anyhow!("ExpirationSet left with negative raw active power"));
        }

        if self.active_power.qa.is_negative() {
            return Err(anyhow!("ExpirationSet left with negative qa active power"));
        }

        if self.faulty_power.raw.is_negative() {
            return Err(anyhow!("ExpirationSet left with negative raw faulty power"));
        }

        if self.faulty_power.qa.is_negative() {
            return Err(anyhow!("ExpirationSet left with negative qa faulty power"));
        }

        Ok(())
    }
}

/// A queue of expiration sets by epoch, representing the on-time or early termination epoch for a collection of sectors.
/// Wraps an AMT[ChainEpoch]*ExpirationSet.
/// Keys in the queue are quantized (upwards), modulo some offset, to reduce the cardinality of keys.
pub struct ExpirationQueue<'db, BS> {
    pub amt: Array<'db, ExpirationSet, BS>,
    pub quant: QuantSpec,
}

impl<'db, BS: Blockstore> ExpirationQueue<'db, BS> {
    /// Loads a queue root.
    ///
    /// Epochs provided to subsequent method calls will be quantized upwards to quanta mod offsetSeed before being
    /// written to/read from queue entries.
    pub fn new(store: &'db BS, root: &Cid, quant: QuantSpec) -> Result<Self, AmtError> {
        Ok(Self {
            amt: Array::load(root, store)?,
            quant,
        })
    }

    /// Adds a collection of sectors to their on-time target expiration entries (quantized).
    /// The sectors are assumed to be active (non-faulty).
    /// Returns the sector numbers, power, and pledge added.
    pub fn add_active_sectors<'a>(
        &mut self,
        sectors: impl IntoIterator<Item = &'a SectorOnChainInfo>,
        sector_size: SectorSize,
    ) -> anyhow::Result<(BitField, PowerPair, TokenAmount)> {
        let mut total_power = PowerPair::zero();
        let mut total_pledge = TokenAmount::zero();
        let mut total_sectors = Vec::<BitField>::new();

        for group in group_new_sectors_by_declared_expiration(sector_size, sectors, self.quant) {
            let sector_numbers = BitField::try_from_bits(group.sectors)?;

            self.add(
                group.epoch,
                &sector_numbers,
                &BitField::new(),
                &group.power,
                &PowerPair::zero(),
                &group.pledge,
            )
            .map_err(|e| e.downcast_wrap("failed to record new sector expirations"))?;

            total_sectors.push(sector_numbers);
            total_power += &group.power;
            total_pledge += &group.pledge;
        }

        let sector_numbers = BitField::union(total_sectors.iter());
        Ok((sector_numbers, total_power, total_pledge))
    }

    /// Reschedules some sectors to a new (quantized) expiration epoch.
    /// The sectors being rescheduled are assumed to be not faulty, and hence are removed from and re-scheduled for on-time
    /// rather than early expiration.
    /// The sectors' power and pledge are assumed not to change, despite the new expiration.
    pub fn reschedule_expirations(
        &mut self,
        new_expiration: ChainEpoch,
        sectors: &[SectorOnChainInfo],
        sector_size: SectorSize,
    ) -> anyhow::Result<()> {
        if sectors.is_empty() {
            return Ok(());
        }

        let (sector_numbers, power, pledge) = self
            .remove_active_sectors(sectors, sector_size)
            .map_err(|e| e.downcast_wrap("failed to remove sector expirations"))?;

        self.add(
            new_expiration,
            &sector_numbers,
            &BitField::new(),
            &power,
            &PowerPair::zero(),
            &pledge,
        )
        .map_err(|e| e.downcast_wrap("failed to record new sector expirations"))?;

        Ok(())
    }

    /// Re-schedules sectors to expire at an early expiration epoch (quantized), if they wouldn't expire before then anyway.
    /// The sectors must not be currently faulty, so must be registered as expiring on-time rather than early.
    /// The pledge for the now-early sectors is removed from the queue.
    /// Returns the total power represented by the sectors.
    pub fn reschedule_as_faults(
        &mut self,
        new_expiration: ChainEpoch,
        sectors: &[SectorOnChainInfo],
        sector_size: SectorSize,
    ) -> anyhow::Result<PowerPair> {
        let mut sectors_total = Vec::new();
        let mut expiring_power = PowerPair::zero();
        let mut rescheduled_power = PowerPair::zero();

        let groups = self.find_sectors_by_expiration(sector_size, sectors)?;

        // Group sectors by their target expiration, then remove from existing queue entries according to those groups.
        for mut group in groups {
            if group.sector_epoch_set.epoch <= self.quant.quantize_up(new_expiration) {
                // Don't reschedule sectors that are already due to expire on-time before the fault-driven expiration,
                // but do represent their power as now faulty.
                // Their pledge remains as "on-time".
                group.expiration_set.active_power -= &group.sector_epoch_set.power;
                group.expiration_set.faulty_power += &group.sector_epoch_set.power;
                expiring_power += &group.sector_epoch_set.power;
            } else {
                // Remove sectors from on-time expiry and active power.
                let sectors_bitfield =
                    BitField::try_from_bits(group.sector_epoch_set.sectors.iter().copied())?;
                group.expiration_set.on_time_sectors -= &sectors_bitfield;
                group.expiration_set.on_time_pledge -= &group.sector_epoch_set.pledge;
                group.expiration_set.active_power -= &group.sector_epoch_set.power;

                // Accumulate the sectors and power removed.
                sectors_total.extend_from_slice(&group.sector_epoch_set.sectors);
                rescheduled_power += &group.sector_epoch_set.power;
            }

            self.must_update_or_delete(group.sector_epoch_set.epoch, group.expiration_set.clone())?;

            group.expiration_set.validate_state()?;
        }

        if !sectors_total.is_empty() {
            // Add sectors to new expiration as early-terminating and faulty.
            let early_sectors = BitField::try_from_bits(sectors_total)?;
            self.add(
                new_expiration,
                &BitField::new(),
                &early_sectors,
                &PowerPair::zero(),
                &rescheduled_power,
                &TokenAmount::zero(),
            )?;
        }

        Ok(&rescheduled_power + &expiring_power)
    }

    /// Re-schedules *all* sectors to expire at an early expiration epoch, if they wouldn't expire before then anyway.
    pub fn reschedule_all_as_faults(&mut self, fault_expiration: ChainEpoch) -> anyhow::Result<()> {
        let mut rescheduled_epochs = Vec::<u64>::new();
        let mut rescheduled_sectors = BitField::new();
        let mut rescheduled_power = PowerPair::zero();

        let mut mutated_expiration_sets = Vec::<(ChainEpoch, ExpirationSet)>::new();

        self.amt.for_each(|e, expiration_set| {
            let epoch: ChainEpoch = e.try_into()?;

            if epoch <= self.quant.quantize_up(fault_expiration) {
                let mut expiration_set = expiration_set.clone();

                // Regardless of whether the sectors were expiring on-time or early, all the power is now faulty.
                // Pledge is still on-time.
                expiration_set.faulty_power += &expiration_set.active_power;
                expiration_set.active_power = PowerPair::zero();
                mutated_expiration_sets.push((epoch, expiration_set));
            } else {
                rescheduled_epochs.push(e);
                // sanity check to make sure we're not trying to re-schedule already faulty sectors.
                if !expiration_set.early_sectors.is_empty() {
                    return Err(anyhow!(
                        "attempted to re-schedule early expirations to an earlier epoch"
                    ));
                }
                rescheduled_sectors |= &expiration_set.on_time_sectors;
                rescheduled_power += &expiration_set.active_power;
                rescheduled_power += &expiration_set.faulty_power;
            }

            Ok(())
        })?;

        for (epoch, expiration_set) in mutated_expiration_sets {
            let res = expiration_set.validate_state();
            self.must_update(epoch, expiration_set)?;
            res?;
        }

        // If we didn't reschedule anything, we're done.
        if rescheduled_epochs.is_empty() {
            return Ok(());
        }

        // Add rescheduled sectors to new expiration as early-terminating and faulty.
        self.add(
            fault_expiration,
            &BitField::new(),
            &rescheduled_sectors,
            &PowerPair::zero(),
            &rescheduled_power,
            &TokenAmount::zero(),
        )?;

        // Trim the rescheduled epochs from the queue.
        self.amt.batch_delete(rescheduled_epochs, true)?;

        Ok(())
    }

    /// Removes sectors from any queue entries in which they appear that are earlier then their scheduled expiration epoch,
    /// and schedules them at their expected termination epoch.
    /// Pledge for the sectors is re-added as on-time.
    /// Power for the sectors is changed from faulty to active (whether rescheduled or not).
    /// Returns the newly-recovered power. Fails if any sectors are not found in the queue.
    pub fn reschedule_recovered(
        &mut self,
        sectors: Vec<SectorOnChainInfo>,
        sector_size: SectorSize,
    ) -> anyhow::Result<PowerPair> {
        let mut remaining: BTreeMap<SectorNumber, &SectorOnChainInfo> = sectors
            .iter()
            .map(|sector| (sector.sector_number, sector))
            .collect();

        // Traverse the expiration queue once to find each recovering sector and remove it from early/faulty there.
        // We expect this to find all recovering sectors within the first FaultMaxAge/WPoStProvingPeriod entries
        // (i.e. 14 for 14-day faults), but if something has gone wrong it's safer not to fail if that's not met.
        let mut sectors_rescheduled = Vec::<&SectorOnChainInfo>::new();
        let mut recovered_power = PowerPair::zero();

        self.iter_while_mut(|_epoch, expiration_set| {
            let mut faulty_power_delta = PowerPair::zero();
            let mut active_power_delta = PowerPair::zero();

            for sector_number in expiration_set.on_time_sectors.iter() {
                let sector = match remaining.remove(&sector_number) {
                    Some(s) => s,
                    None => continue,
                };

                // If the sector expires on-time at this epoch, leave it here but change faulty power to active.
                // The pledge is already part of the on-time pledge at this entry.
                let power = power_for_sector(sector_size, sector);
                faulty_power_delta -= &power;
                active_power_delta += &power;

                recovered_power += &power;
            }
            let mut early_unset = Vec::new();
            for sector_number in expiration_set.early_sectors.iter() {
                let sector = match remaining.remove(&sector_number) {
                    Some(s) => s,
                    None => continue,
                };
                // If the sector expires early at this epoch, remove it for re-scheduling.
                // It's not part of the on-time pledge number here.
                early_unset.push(sector_number);
                let power = power_for_sector(sector_size, sector);
                faulty_power_delta -= &power;
                sectors_rescheduled.push(sector);

                recovered_power += &power;
            }

            // we need to defer the changes as we cannot borrow immutably for iteration
            // and mutably for changes at the same time
            if !early_unset.is_empty()
                || !faulty_power_delta.is_zero()
                || !active_power_delta.is_zero()
            {
                expiration_set.active_power += &active_power_delta;
                expiration_set.faulty_power += &faulty_power_delta;

                expiration_set.early_sectors -= BitField::try_from_bits(early_unset)?;
            }

            expiration_set.validate_state()?;

            let keep_going = !remaining.is_empty();
            Ok(keep_going)
        })?;

        if !remaining.is_empty() {
            return Err(anyhow!(
                "sectors not found in expiration queue: {:?}",
                remaining
            ));
        }

        // Re-schedule the removed sectors to their target expiration.
        self.add_active_sectors(sectors_rescheduled, sector_size)?;

        Ok(recovered_power)
    }

    /// Removes some sectors and adds some others.
    /// The sectors being replaced must not be faulty, so must be scheduled for on-time rather than early expiration.
    /// The sectors added are assumed to be not faulty.
    /// Returns the old a new sector number bitfields, and delta to power and pledge, new minus old.
    pub fn replace_sectors(
        &mut self,
        old_sectors: &[SectorOnChainInfo],
        new_sectors: &[SectorOnChainInfo],
        sector_size: SectorSize,
    ) -> anyhow::Result<(BitField, BitField, PowerPair, TokenAmount)> {
        let (old_sector_numbers, old_power, old_pledge) = self
            .remove_active_sectors(old_sectors, sector_size)
            .map_err(|e| e.downcast_wrap("failed to remove replaced sectors"))?;

        let (new_sector_numbers, new_power, new_pledge) = self
            .add_active_sectors(new_sectors, sector_size)
            .map_err(|e| e.downcast_wrap("failed to add replacement sectors"))?;

        Ok((
            old_sector_numbers,
            new_sector_numbers,
            &new_power - &old_power,
            new_pledge - old_pledge,
        ))
    }

    /// Remove some sectors from the queue.
    /// The sectors may be active or faulty, and scheduled either for on-time or early termination.
    /// Returns the aggregate of removed sectors and power, and recovering power.
    /// Fails if any sectors are not found in the queue.
    pub fn remove_sectors(
        &mut self,
        policy: &Policy,
        sectors: &[SectorOnChainInfo],
        faults: &BitField,
        recovering: &BitField,
        sector_size: SectorSize,
    ) -> anyhow::Result<(ExpirationSet, PowerPair)> {
        let mut remaining: BTreeSet<_> =
            sectors.iter().map(|sector| sector.sector_number).collect();

        // ADDRESSED_SECTORS_MAX is defined as 25000, so this will not error.
        let faults_map: BTreeSet<_> = faults
            .bounded_iter(policy.addressed_sectors_max)
            .context("too many faults to expand")?
            .map(|i| i as SectorNumber)
            .collect();

        let recovering_map: BTreeSet<_> = recovering
            .bounded_iter(policy.addressed_sectors_max)
            .context("too many recoveries to expand")?
            .map(|i| i as SectorNumber)
            .collect();

        // results
        let mut removed = ExpirationSet::empty();
        let mut recovering_power = PowerPair::zero();

        // Split into faulty and non-faulty. We process non-faulty sectors first
        // because they always expire on-time so we know where to find them.
        // TODO since cloning info, should be RC or find a way for data to be references.
        // This might get optimized by the compiler, so not a priority
        let mut non_faulty_sectors = Vec::<SectorOnChainInfo>::new();
        let mut faulty_sectors = Vec::<&SectorOnChainInfo>::new();

        for sector in sectors {
            if faults_map.contains(&sector.sector_number) {
                faulty_sectors.push(sector);
            } else {
                non_faulty_sectors.push(sector.clone());

                // remove them from "remaining", we're going to process them below.
                remaining.remove(&sector.sector_number);
            }
        }

        // Remove non-faulty sectors.
        let (removed_sector_numbers, removed_power, removed_pledge) = self
            .remove_active_sectors(&non_faulty_sectors, sector_size)
            .map_err(|e| e.downcast_wrap("failed to remove on-time recoveries"))?;
        removed.on_time_sectors = removed_sector_numbers;
        removed.active_power = removed_power;
        removed.on_time_pledge = removed_pledge;

        // Finally, remove faulty sectors (on time and not). These sectors can
        // only appear within the first 14 days (fault max age). Given that this
        // queue is quantized, we should be able to stop traversing the queue
        // after 14 entries.
        self.iter_while_mut(|_epoch, expiration_set| {
            let on_time_sectors: BTreeSet<SectorNumber> = expiration_set
                .on_time_sectors
                .bounded_iter(ENTRY_SECTORS_MAX)
                .context("too many on-time sectors to expand")?
                .map(|i| i as SectorNumber)
                .collect();

            let early_sectors: BTreeSet<SectorNumber> = expiration_set
                .early_sectors
                .bounded_iter(ENTRY_SECTORS_MAX)
                .context("too many early sectors to expand")?
                .map(|i| i as SectorNumber)
                .collect();

            // This loop could alternatively be done by constructing bitfields and intersecting them, but it's not
            // clear that would be much faster (O(max(N, M)) vs O(N+M)).
            // The length of sectors has a maximum of one partition size.
            for sector in &faulty_sectors {
                let sector_number = sector.sector_number;
                let mut found = false;

                if on_time_sectors.contains(&sector_number) {
                    found = true;
                    expiration_set.on_time_sectors.unset(sector_number);
                    removed.on_time_sectors.set(sector_number);
                    expiration_set.on_time_pledge -= &sector.initial_pledge;
                    removed.on_time_pledge += &sector.initial_pledge;
                } else if early_sectors.contains(&sector_number) {
                    found = true;
                    expiration_set.early_sectors.unset(sector_number);
                    removed.early_sectors.set(sector_number);
                }

                if found {
                    let power = power_for_sector(sector_size, sector);

                    if faults_map.contains(&sector_number) {
                        expiration_set.faulty_power -= &power;
                        removed.faulty_power += &power;
                    } else {
                        expiration_set.active_power -= &power;
                        removed.active_power += &power;
                    }

                    if recovering_map.contains(&sector_number) {
                        recovering_power += &power;
                    }

                    remaining.remove(&sector_number);
                }
            }

            expiration_set.validate_state()?;

            let keep_going = !remaining.is_empty();
            Ok(keep_going)
        })?;

        if !remaining.is_empty() {
            return Err(anyhow!(
                "sectors not found in expiration queue: {:?}",
                remaining
            ));
        }

        Ok((removed, recovering_power))
    }

    /// Removes and aggregates entries from the queue up to and including some epoch.
    pub fn pop_until(&mut self, until: ChainEpoch) -> anyhow::Result<ExpirationSet> {
        let mut on_time_sectors = BitField::new();
        let mut early_sectors = BitField::new();
        let mut active_power = PowerPair::zero();
        let mut faulty_power = PowerPair::zero();
        let mut on_time_pledge = TokenAmount::zero();
        let mut popped_keys = Vec::<u64>::new();

        self.amt.for_each_while(|i, this_value| {
            if i as ChainEpoch > until {
                return Ok(false);
            }

            popped_keys.push(i);
            on_time_sectors |= &this_value.on_time_sectors;
            early_sectors |= &this_value.early_sectors;
            active_power += &this_value.active_power;
            faulty_power += &this_value.faulty_power;
            on_time_pledge += &this_value.on_time_pledge;

            Ok(true)
        })?;

        self.amt.batch_delete(popped_keys, true)?;

        Ok(ExpirationSet {
            on_time_sectors,
            early_sectors,
            on_time_pledge,
            active_power,
            faulty_power,
        })
    }

    fn add(
        &mut self,
        raw_epoch: ChainEpoch,
        on_time_sectors: &BitField,
        early_sectors: &BitField,
        active_power: &PowerPair,
        faulty_power: &PowerPair,
        pledge: &TokenAmount,
    ) -> anyhow::Result<()> {
        let epoch = self.quant.quantize_up(raw_epoch);
        let mut expiration_set = self.may_get(epoch)?;

        expiration_set
            .add(
                on_time_sectors,
                early_sectors,
                pledge,
                active_power,
                faulty_power,
            )
            .map_err(|e| anyhow!("failed to add expiration values for epoch {}: {}", epoch, e))?;

        self.must_update(epoch, expiration_set)?;
        Ok(())
    }

    fn remove(
        &mut self,
        raw_epoch: ChainEpoch,
        on_time_sectors: &BitField,
        early_sectors: &BitField,
        active_power: &PowerPair,
        faulty_power: &PowerPair,
        pledge: &TokenAmount,
    ) -> anyhow::Result<()> {
        let epoch = self.quant.quantize_up(raw_epoch);
        let mut expiration_set = self
            .amt
            .get(epoch.try_into()?)
            .map_err(|e| e.downcast_wrap(format!("failed to lookup queue epoch {}", epoch)))?
            .ok_or_else(|| anyhow!("missing expected expiration set at epoch {}", epoch))?
            .clone();
        expiration_set
            .remove(
                on_time_sectors,
                early_sectors,
                pledge,
                active_power,
                faulty_power,
            )
            .map_err(|e| {
                anyhow!(
                    "failed to remove expiration values for queue epoch {}: {}",
                    epoch,
                    e
                )
            })?;

        self.must_update_or_delete(epoch, expiration_set)?;
        Ok(())
    }

    fn remove_active_sectors(
        &mut self,
        sectors: &[SectorOnChainInfo],
        sector_size: SectorSize,
    ) -> anyhow::Result<(BitField, PowerPair, TokenAmount)> {
        let mut removed_sector_numbers = Vec::<u64>::new();
        let mut removed_power = PowerPair::zero();
        let mut removed_pledge = TokenAmount::zero();

        // Group sectors by their expiration, then remove from existing queue entries according to those groups.
        let groups = self.find_sectors_by_expiration(sector_size, sectors)?;
        for group in groups {
            let sectors_bitfield =
                BitField::try_from_bits(group.sector_epoch_set.sectors.iter().copied())?;
            self.remove(
                group.sector_epoch_set.epoch,
                &sectors_bitfield,
                &BitField::new(),
                &group.sector_epoch_set.power,
                &PowerPair::zero(),
                &group.sector_epoch_set.pledge,
            )?;

            removed_sector_numbers.extend(&group.sector_epoch_set.sectors);

            removed_power += &group.sector_epoch_set.power;
            removed_pledge += &group.sector_epoch_set.pledge;
        }

        Ok((
            BitField::try_from_bits(removed_sector_numbers)?,
            removed_power,
            removed_pledge,
        ))
    }

    /// Traverses the entire queue with a callback function that may mutate entries.
    /// Iff the function returns that it changed an entry, the new entry will be re-written in the queue. Any changed
    /// entries that become empty are removed after iteration completes.
    fn iter_while_mut(
        &mut self,
        mut f: impl FnMut(
            ChainEpoch,
            &mut ValueMut<'_, ExpirationSet>,
        ) -> anyhow::Result</* keep going */ bool>,
    ) -> anyhow::Result<()> {
        let mut epochs_emptied = Vec::<u64>::new();

        self.amt.for_each_while_mut(|e, expiration_set| {
            let keep_going = f(e.try_into()?, expiration_set)?;

            if expiration_set.is_empty() {
                // Mark expiration set as unchanged, it will be removed after the iteration.
                expiration_set.mark_unchanged();
                epochs_emptied.push(e);
            }

            Ok(keep_going)
        })?;

        self.amt.batch_delete(epochs_emptied, true)?;

        Ok(())
    }

    fn may_get(&self, key: ChainEpoch) -> anyhow::Result<ExpirationSet> {
        Ok(self
            .amt
            .get(key.try_into()?)
            .map_err(|e| e.downcast_wrap(format!("failed to lookup queue epoch {}", key)))?
            .cloned()
            .unwrap_or_default())
    }

    fn must_update(
        &mut self,
        epoch: ChainEpoch,
        expiration_set: ExpirationSet,
    ) -> anyhow::Result<()> {
        self.amt
            .set(epoch.try_into()?, expiration_set)
            .map_err(|e| e.downcast_wrap(format!("failed to set queue epoch {}", epoch)))
    }

    /// Since this might delete the node, it's not safe for use inside an iteration.
    fn must_update_or_delete(
        &mut self,
        epoch: ChainEpoch,
        expiration_set: ExpirationSet,
    ) -> anyhow::Result<()> {
        if expiration_set.is_empty() {
            self.amt
                .delete(epoch.try_into()?)
                .map_err(|e| e.downcast_wrap(format!("failed to delete queue epoch {}", epoch)))?;
        } else {
            self.amt
                .set(epoch.try_into()?, expiration_set)
                .map_err(|e| e.downcast_wrap(format!("failed to set queue epoch {}", epoch)))?;
        }

        Ok(())
    }

    /// Groups sectors into sets based on their Expiration field.
    /// If sectors are not found in the expiration set corresponding to their expiration field
    /// (i.e. they have been rescheduled) traverse expiration sets for groups where these
    /// sectors actually expire.
    /// Groups will be returned in expiration order, earliest first.
    fn find_sectors_by_expiration(
        &self,
        sector_size: SectorSize,
        sectors: &[SectorOnChainInfo],
    ) -> anyhow::Result<Vec<SectorExpirationSet>> {
        let mut declared_expirations = BTreeMap::<ChainEpoch, bool>::new();
        let mut sectors_by_number = BTreeMap::<u64, &SectorOnChainInfo>::new();
        let mut all_remaining = BTreeSet::<u64>::new();
        let mut expiration_groups =
            Vec::<SectorExpirationSet>::with_capacity(declared_expirations.len());

        for sector in sectors {
            let q_expiration = self.quant.quantize_up(sector.expiration);
            declared_expirations.insert(q_expiration, true);
            all_remaining.insert(sector.sector_number);
            sectors_by_number.insert(sector.sector_number, sector);
        }

        for (&expiration, _) in declared_expirations.iter() {
            let es = self.may_get(expiration)?;

            let group = group_expiration_set(
                sector_size,
                &sectors_by_number,
                &mut all_remaining,
                es,
                expiration,
            );
            if !group.sector_epoch_set.sectors.is_empty() {
                expiration_groups.push(group);
            }
        }

        // If sectors remain, traverse next in epoch order. Remaining sectors should be
        // rescheduled to expire soon, so this traversal should exit early.
        if !all_remaining.is_empty() {
            self.amt.for_each_while(|epoch, es| {
                let epoch = epoch as ChainEpoch;
                // If this set's epoch is one of our declared epochs, we've already processed it
                // in the loop above, so skip processing here. Sectors rescheduled to this epoch
                // would have been included in the earlier processing.
                if declared_expirations.contains_key(&epoch) {
                    return Ok(true);
                }

                // Sector should not be found in EarlyExpirations which holds faults. An implicit assumption
                // of grouping is that it only returns sectors with active power. ExpirationQueue should not
                // provide operations that allow this to happen.
                check_no_early_sectors(&all_remaining, es)?;

                let group = group_expiration_set(
                    sector_size,
                    &sectors_by_number,
                    &mut all_remaining,
                    es.clone(),
                    epoch,
                );

                if !group.sector_epoch_set.sectors.is_empty() {
                    expiration_groups.push(group);
                }

                Ok(!all_remaining.is_empty())
            })?;
        }

        if !all_remaining.is_empty() {
            return Err(anyhow!("some sectors not found in expiration queue"));
        }

        // The built-in stable sort is timsort. It will find the two sorted runs and merge them.
        //
        // We could also just assume they're sorted and use itertools.merge..., but this is safe and
        // the perf shouldn't be much different.
        expiration_groups.sort_by_key(|g| g.sector_epoch_set.epoch);

        Ok(expiration_groups)
    }
}

#[derive(Clone)]
struct SectorExpirationSet {
    sector_epoch_set: SectorEpochSet,
    // TODO try to make expiration set a reference (or Cow)
    expiration_set: ExpirationSet,
}

#[derive(Clone)]
struct SectorEpochSet {
    epoch: ChainEpoch,
    sectors: Vec<u64>,
    power: PowerPair,
    pledge: TokenAmount,
}

/// Takes a slice of sector infos and returns sector info sets grouped and
/// sorted by expiration epoch, quantized.
///
/// Note: While the result is sorted by epoch, the order of per-epoch sectors is maintained.
fn group_new_sectors_by_declared_expiration<'a>(
    sector_size: SectorSize,
    sectors: impl IntoIterator<Item = &'a SectorOnChainInfo>,
    quant: QuantSpec,
) -> Vec<SectorEpochSet> {
    let mut sectors_by_expiration = BTreeMap::<ChainEpoch, Vec<&SectorOnChainInfo>>::new();

    for sector in sectors {
        let q_expiration = quant.quantize_up(sector.expiration);
        sectors_by_expiration
            .entry(q_expiration)
            .or_default()
            .push(sector);
    }

    // The result is sorted by expiration because the BTreeMap iterates in sorted order.
    sectors_by_expiration
        .into_iter()
        .map(|(expiration, epoch_sectors)| {
            let mut sector_numbers = Vec::<u64>::with_capacity(epoch_sectors.len());
            let mut total_power = PowerPair::zero();
            let mut total_pledge = TokenAmount::zero();

            for sector in epoch_sectors {
                sector_numbers.push(sector.sector_number);
                total_power += &power_for_sector(sector_size, sector);
                total_pledge += &sector.initial_pledge;
            }

            SectorEpochSet {
                epoch: expiration,
                sectors: sector_numbers,
                power: total_power,
                pledge: total_pledge,
            }
        })
        .collect()
}

fn group_expiration_set(
    sector_size: SectorSize,
    sectors: &BTreeMap<u64, &SectorOnChainInfo>,
    include_set: &mut BTreeSet<u64>,
    es: ExpirationSet,
    expiration: ChainEpoch,
) -> SectorExpirationSet {
    let mut sector_numbers = Vec::new();
    let mut total_power = PowerPair::zero();
    let mut total_pledge = TokenAmount::default();

    for u in es.on_time_sectors.iter() {
        if include_set.remove(&u) {
            let sector = sectors.get(&u).expect("index should exist in sector set");
            sector_numbers.push(u);
            total_power += &power_for_sector(sector_size, sector);
            total_pledge += &sector.initial_pledge;
        }
    }

    SectorExpirationSet {
        sector_epoch_set: SectorEpochSet {
            epoch: expiration,
            sectors: sector_numbers,
            power: total_power,
            pledge: total_pledge,
        },
        expiration_set: es,
    }
}

/// Checks for invalid overlap between bitfield and a set's early sectors.
fn check_no_early_sectors(set: &BTreeSet<u64>, es: &ExpirationSet) -> anyhow::Result<()> {
    for u in es.early_sectors.iter() {
        if set.contains(&(u)) {
            return Err(anyhow!(
                "Invalid attempt to group sector {} with an early expiration",
                u
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
