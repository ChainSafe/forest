// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{power_for_sector, PowerPair, QuantSpec, SectorOnChainInfo, SECTORS_MAX};
use bitfield::BitField;
use cid::Cid;
use clock::ChainEpoch;
use encoding::tuple::*;
use fil_types::{SectorNumber, SectorSize};
use ipld_amt::{Amt, Error as AmtError};
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser;
use num_traits::{Signed, Zero};
use std::{collections::HashMap, collections::HashSet, error::Error as StdError};
use vm::TokenAmount;

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
    #[serde(with = "bigint_ser")]
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
    ) {
        self.on_time_sectors |= on_time_sectors;
        self.early_sectors |= early_sectors;
        self.on_time_pledge += on_time_pledge;
        self.active_power += active_power;
        self.faulty_power += faulty_power;
    }

    /// Removes sectors and power from the expiration set in place.
    pub fn remove(
        &mut self,
        on_time_sectors: &BitField,
        early_sectors: &BitField,
        on_time_pledge: &TokenAmount,
        active_power: &PowerPair,
        faulty_power: &PowerPair,
    ) -> Result<(), String> {
        // Check for sector intersection. This could be cheaper with a combined intersection/difference method used below.
        if !self.on_time_sectors.contains_all(on_time_sectors) {
            return Err(format!(
                "removing on-time sectors {:?} not contained in {:?}",
                on_time_sectors, self.on_time_sectors
            ));
        }
        if !self.early_sectors.contains_all(early_sectors) {
            return Err(format!(
                "removing early sectors {:?} not contained in {:?}",
                early_sectors, self.early_sectors
            ));
        }

        self.on_time_sectors -= on_time_sectors;
        self.early_sectors -= early_sectors;
        self.on_time_pledge -= on_time_pledge;
        self.active_power -= active_power;
        self.faulty_power -= faulty_power;

        // Check underflow.
        if self.on_time_pledge.is_negative() {
            return Err(format!("expiration set pledge underflow: {:?}", self));
        }
        if self.active_power.qa.is_negative() || self.faulty_power.qa.is_negative() {
            return Err(format!("expiration set power underflow: {:?}", self));
        }

        Ok(())
    }

    /// A set is empty if it has no sectors.
    /// The power and pledge are not checked, but are expected to be zero.
    pub fn is_empty(&self) -> bool {
        self.on_time_sectors.is_empty() && self.early_sectors.is_empty()
    }

    /// Counts all sectors in the expiration set.
    pub fn len(&self) -> usize {
        self.on_time_sectors.len() + self.early_sectors.len()
    }
}

/// A queue of expiration sets by epoch, representing the on-time or early termination epoch for a collection of sectors.
/// Wraps an AMT[ChainEpoch]*ExpirationSet.
/// Keys in the queue are quantized (upwards), modulo some offset, to reduce the cardinality of keys.
pub struct ExpirationQueue<'db, BS> {
    pub amt: Amt<'db, ExpirationSet, BS>,
    pub quant: QuantSpec,
}

impl<'db, BS: BlockStore> ExpirationQueue<'db, BS> {
    /// Loads a queue root.
    ///
    /// Epochs provided to subsequent method calls will be quantized upwards to quanta mod offsetSeed before being
    /// written to/read from queue entries.
    pub fn new(store: &'db BS, root: &Cid, quant: QuantSpec) -> Result<Self, AmtError> {
        Ok(Self {
            amt: Amt::load(root, store)?,
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
    ) -> Result<(BitField, PowerPair, TokenAmount), String> {
        let mut total_power = PowerPair::zero();
        let mut total_pledge = TokenAmount::zero();
        let mut total_sectors = Vec::<BitField>::new();

        for group in group_sectors_by_expiration(sector_size, sectors, self.quant) {
            let sector_numbers: BitField = group.sectors.iter().map(|&i| i as usize).collect();

            self.add(
                group.epoch,
                &sector_numbers,
                &BitField::new(),
                &group.power,
                &PowerPair::zero(),
                &group.pledge,
            )
            .map_err(|e| format!("failed to record new sector expirations: {:?}", e))?;

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
    ) -> Result<(), String> {
        if sectors.is_empty() {
            return Ok(());
        }

        let (sector_numbers, power, pledge) = self
            .remove_active_sectors(sectors, sector_size)
            .map_err(|e| format!("failed to remove sector expirations: {:?}", e))?;

        self.add(
            new_expiration,
            &sector_numbers,
            &BitField::new(),
            &power,
            &PowerPair::zero(),
            &pledge,
        )
        .map_err(|e| format!("failed to record new sector expirations: {}", e))?;

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
    ) -> Result<PowerPair, String> {
        let mut sectors_total = Vec::new();
        let mut expiring_power = PowerPair::zero();
        let mut rescheduled_power = PowerPair::zero();

        // Group sectors by their target expiration, then remove from existing queue entries according to those groups.
        for group in group_sectors_by_expiration(sector_size, sectors, self.quant) {
            let mut expiration_set = self.must_get(group.epoch)?;

            if group.epoch <= self.quant.quantize_up(new_expiration) {
                // Don't reschedule sectors that are already due to expire on-time before the fault-driven expiration,
                // but do represent their power as now faulty.
                // Their pledge remains as "on-time".
                expiration_set.active_power -= &group.power;
                expiration_set.faulty_power += &group.power;
                expiring_power += &group.power;
            } else {
                // Remove sectors from on-time expiry and active power.
                let sectors_bitfield: BitField =
                    group.sectors.iter().map(|&i| i as usize).collect();
                expiration_set.on_time_sectors -= &sectors_bitfield;
                expiration_set.on_time_pledge -= &group.pledge;
                expiration_set.active_power -= &group.power;

                // Accumulate the sectors and power removed.
                sectors_total.extend_from_slice(&group.sectors);
                rescheduled_power += &group.power;
            }

            self.must_update_or_delete(group.epoch, expiration_set)?;
        }

        if !sectors_total.is_empty() {
            // Add sectors to new expiration as early-terminating and faulty.
            let early_sectors: BitField = sectors_total.iter().map(|&i| i as usize).collect();
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
    pub fn reschedule_all_as_faults(
        &mut self,
        fault_expiration: ChainEpoch,
    ) -> Result<(), Box<dyn StdError>> {
        let mut rescheduled_epochs = Vec::<u64>::new();
        let mut rescheduled_sectors = BitField::new();
        let mut rescheduled_power = PowerPair::zero();

        let mut mutated_expiration_sets = Vec::<(ChainEpoch, ExpirationSet)>::new();

        self.amt.for_each(|e, expiration_set| {
            let epoch = e as ChainEpoch;

            if epoch <= self.quant.quantize_up(fault_expiration) {
                let mut expiration_set = expiration_set.clone();

                // Regardless of whether the sectors were expiring on-time or early, all the power is now faulty.
                // Pledge is still on-time.
                expiration_set.faulty_power += &expiration_set.active_power;
                expiration_set.active_power = PowerPair::zero();
                mutated_expiration_sets.push((epoch, expiration_set));
            } else {
                rescheduled_epochs.push(epoch as u64);
                rescheduled_sectors |= &expiration_set.on_time_sectors;
                rescheduled_sectors |= &expiration_set.early_sectors;
                rescheduled_power += &expiration_set.active_power;
                rescheduled_power += &expiration_set.faulty_power;
            }

            Ok(())
        })?;

        for (epoch, expiration_set) in mutated_expiration_sets {
            self.must_update(epoch, expiration_set)?;
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
        self.amt.batch_delete(rescheduled_epochs)?;

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
    ) -> Result<PowerPair, Box<dyn StdError>> {
        let mut remaining: HashSet<SectorNumber> =
            sectors.iter().map(|sector| sector.sector_number).collect();

        // Traverse the expiration queue once to find each recovering sector and remove it from early/faulty there.
        // We expect this to find all recovering sectors within the first FaultMaxAge/WPoStProvingPeriod entries
        // (i.e. 14 for 14-day faults), but if something has gone wrong it's safer not to fail if that's not met.
        let mut sectors_rescheduled = Vec::<&SectorOnChainInfo>::new();
        let mut recovered_power = PowerPair::zero();

        self.iter_while_mut(|_epoch, expiration_set| {
            let on_time_sectors: HashSet<SectorNumber> = expiration_set
                .on_time_sectors
                .bounded_iter(SECTORS_MAX as usize)?
                .map(|i| i as SectorNumber)
                .collect();

            let early_sectors: HashSet<SectorNumber> = expiration_set
                .early_sectors
                .bounded_iter(SECTORS_MAX as usize)?
                .map(|i| i as SectorNumber)
                .collect();

            // This loop could alternatively be done by constructing bitfields and intersecting them, but it's not
            // clear that would be much faster (O(max(N, M)) vs O(N+M)).
            // If faults are correlated, the first queue entry likely has them all anyway.
            // The length of sectors has a maximum of one partition size.
            for sector in sectors.iter() {
                let sector_number = sector.sector_number;
                let power = power_for_sector(sector_size, sector);
                let mut found = false;

                if on_time_sectors.contains(&sector_number) {
                    found = true;
                    // If the sector expires on-time at this epoch, leave it here but change faulty power to active.
                    // The pledge is already part of the on-time pledge at this entry.
                    expiration_set.faulty_power -= &power;
                    expiration_set.active_power += &power;
                } else if early_sectors.contains(&sector_number) {
                    found = true;
                    // If the sector expires early at this epoch, remove it for re-scheduling.
                    // It's not part of the on-time pledge number here.
                    expiration_set.early_sectors.unset(sector_number as usize);
                    expiration_set.faulty_power -= &power;
                    sectors_rescheduled.push(sector);
                }

                if found {
                    recovered_power += &power;
                    remaining.remove(&sector.sector_number);
                }
            }

            let keep_going = !remaining.is_empty();
            Ok(keep_going)
        })?;

        if !remaining.is_empty() {
            return Err(format!("sectors not found in expiration queue: {:?}", remaining).into());
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
    ) -> Result<(BitField, BitField, PowerPair, TokenAmount), String> {
        let (old_sector_numbers, old_power, old_pledge) = self
            .remove_active_sectors(old_sectors, sector_size)
            .map_err(|e| format!("failed to remove replaced sectors: {}", e))?;

        let (new_sector_numbers, new_power, new_pledge) = self
            .add_active_sectors(new_sectors, sector_size)
            .map_err(|e| format!("failed to add replacement sectors: {}", e))?;

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
        sectors: &[SectorOnChainInfo],
        faults: &BitField,
        recovering: &BitField,
        sector_size: SectorSize,
    ) -> Result<(ExpirationSet, PowerPair), Box<dyn StdError>> {
        let mut remaining: HashSet<_> = sectors.iter().map(|sector| sector.sector_number).collect();

        let faults_map: HashSet<_> = faults
            .bounded_iter(SECTORS_MAX as usize)
            .map_err(|e| format!("failed to expand faults: {}", e))?
            .map(|i| i as SectorNumber)
            .collect();

        let recovering_map: HashSet<_> = recovering
            .bounded_iter(SECTORS_MAX as usize)
            .map_err(|e| format!("failed to expand recoveries: {}", e))?
            .map(|i| i as SectorNumber)
            .collect();

        // results
        let mut removed = ExpirationSet::empty();
        let mut recovering_power = PowerPair::zero();

        // Split into faulty and non-faulty. We process non-faulty sectors first
        // because they always expire on-time so we know where to find them.
        let mut non_faulty_sectors = Vec::<&SectorOnChainInfo>::new();
        let mut faulty_sectors = Vec::<&SectorOnChainInfo>::new();

        for sector in sectors {
            if faults_map.contains(&sector.sector_number) {
                faulty_sectors.push(sector);
            } else {
                non_faulty_sectors.push(sector);

                // remove them from "remaining", we're going to process them below.
                remaining.remove(&sector.sector_number);
            }
        }

        // Remove non-faulty sectors.
        let (removed_sector_numbers, removed_power, removed_pledge) = self
            .remove_active_sectors(sectors, sector_size)
            .map_err(|e| format!("failed to remove on-time recoveries: {}", e))?;
        removed.on_time_sectors = removed_sector_numbers;
        removed.active_power = removed_power;
        removed.on_time_pledge = removed_pledge;

        // Finally, remove faulty sectors (on time and not). These sectors can
        // only appear within the first 14 days (fault max age). Given that this
        // queue is quantized, we should be able to stop traversing the queue
        // after 14 entries.
        self.iter_while_mut(|_epoch, expiration_set| {
            let on_time_sectors: HashSet<SectorNumber> = expiration_set
                .on_time_sectors
                .bounded_iter(SECTORS_MAX as usize)?
                .map(|i| i as SectorNumber)
                .collect();

            let early_sectors: HashSet<SectorNumber> = expiration_set
                .early_sectors
                .bounded_iter(SECTORS_MAX as usize)?
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
                    expiration_set.on_time_sectors.unset(sector_number as usize);
                    removed.on_time_sectors.set(sector_number as usize);
                    expiration_set.on_time_pledge -= &sector.initial_pledge;
                    removed.on_time_pledge += &sector.initial_pledge;
                } else if early_sectors.contains(&sector_number) {
                    found = true;
                    expiration_set.early_sectors.unset(sector_number as usize);
                    removed.early_sectors.set(sector_number as usize);
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

            let keep_going = !remaining.is_empty();
            Ok(keep_going)
        })?;

        if !remaining.is_empty() {
            return Err(format!("sectors not found in expiration queue: {:?}", remaining).into());
        }

        Ok((removed, recovering_power))
    }

    /// Removes and aggregates entries from the queue up to and including some epoch.
    pub fn pop_until(&mut self, until: ChainEpoch) -> Result<ExpirationSet, Box<dyn StdError>> {
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

        self.amt.batch_delete(popped_keys)?;

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
    ) -> Result<(), String> {
        let epoch = self.quant.quantize_up(raw_epoch);
        let mut expiration_set = self.may_get(epoch)?;

        expiration_set.add(
            on_time_sectors,
            early_sectors,
            pledge,
            active_power,
            faulty_power,
        );

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
    ) -> Result<(), String> {
        let epoch = self.quant.quantize_up(raw_epoch);
        let mut expiration_set = self.must_get(epoch)?;
        expiration_set
            .remove(
                on_time_sectors,
                early_sectors,
                pledge,
                active_power,
                faulty_power,
            )
            .map_err(|e| {
                format!(
                    "failed to remove expiration values for queue epoch {}: {}",
                    epoch, e
                )
            })?;

        self.must_update_or_delete(epoch, expiration_set)?;
        Ok(())
    }

    fn remove_active_sectors(
        &mut self,
        sectors: &[SectorOnChainInfo],
        sector_size: SectorSize,
    ) -> Result<(BitField, PowerPair, TokenAmount), String> {
        let mut removed_sector_numbers = BitField::new();
        let mut removed_power = PowerPair::zero();
        let mut removed_pledge = TokenAmount::zero();

        // Group sectors by their expiration, then remove from existing queue entries according to those groups.
        for group in group_sectors_by_expiration(sector_size, sectors, self.quant) {
            let sectors_bitfield: BitField = group.sectors.iter().map(|&i| i as usize).collect();
            self.remove(
                group.epoch,
                &sectors_bitfield,
                &BitField::new(),
                &group.power,
                &PowerPair::zero(),
                &group.pledge,
            )?;

            for n in group.sectors {
                removed_sector_numbers.set(n as usize);
            }

            removed_power += &group.power;
            removed_pledge += &group.pledge;
        }

        Ok((removed_sector_numbers, removed_power, removed_pledge))
    }

    /// Traverses the entire queue with a callback function that may mutate entries.
    /// Iff the function returns that it changed an entry, the new entry will be re-written in the queue. Any changed
    /// entries that become empty are removed after iteration completes.
    fn iter_while_mut(
        &mut self,
        mut f: impl FnMut(
            ChainEpoch,
            &mut ExpirationSet,
        ) -> Result</* keep going */ bool, Box<dyn StdError>>,
    ) -> Result<(), Box<dyn StdError>> {
        let mut epochs_emptied = Vec::<ChainEpoch>::new();

        self.amt.for_each_while_mut(|e, expiration_set| {
            let epoch = e as ChainEpoch;
            let keep_going = f(epoch, expiration_set)?;

            if expiration_set.is_empty() {
                epochs_emptied.push(epoch);
            }

            Ok(keep_going)
        })?;

        self.amt
            .batch_delete(epochs_emptied.iter().map(|&i| i as u64))?;

        Ok(())
    }

    fn may_get(&self, key: ChainEpoch) -> Result<ExpirationSet, String> {
        Ok(self
            .amt
            .get(key as u64)
            .map_err(|e| format!("failed to lookup queue epoch {}: {:?}", key, e))?
            .unwrap_or_default())
    }

    fn must_get(&self, key: ChainEpoch) -> Result<ExpirationSet, String> {
        self.amt
            .get(key as u64)
            .map_err(|e| format!("failed to lookup queue epoch {}: {:?}", key, e))?
            .ok_or_else(|| format!("missing expected expiration set at epoch {}", key))
    }

    fn must_update(
        &mut self,
        epoch: ChainEpoch,
        expiration_set: ExpirationSet,
    ) -> Result<(), String> {
        self.amt
            .set(epoch as u64, expiration_set)
            .map_err(|e| format!("failed to set queue epoch {}: {:?}", epoch, e))
    }

    /// Since this might delete the node, it's not safe for use inside an iteration.
    fn must_update_or_delete(
        &mut self,
        epoch: ChainEpoch,
        expiration_set: ExpirationSet,
    ) -> Result<(), String> {
        if expiration_set.is_empty() {
            self.amt
                .delete(epoch as u64)
                .map_err(|e| format!("failed to delete queue epoch {}: {:?}", epoch, e))?;
        } else {
            self.amt
                .set(epoch as u64, expiration_set)
                .map_err(|e| format!("failed to set queue epoch {}: {:?}", epoch, e))?;
        }

        Ok(())
    }
}

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
fn group_sectors_by_expiration<'a>(
    sector_size: SectorSize,
    sectors: impl IntoIterator<Item = &'a SectorOnChainInfo>,
    quant: QuantSpec,
) -> Vec<SectorEpochSet> {
    let mut sectors_by_expiration = HashMap::<ChainEpoch, Vec<&SectorOnChainInfo>>::new();

    for sector in sectors {
        let q_expiration = quant.quantize_up(sector.expiration);
        sectors_by_expiration
            .entry(q_expiration)
            .or_default()
            .push(sector);
    }

    // This map iteration is non-deterministic but safe because we sort by epoch below.
    let mut sector_epoch_sets: Vec<_> = sectors_by_expiration
        .into_iter()
        .map(|(expiration, epoch_sectors)| {
            let mut sector_numbers = Vec::<u64>::with_capacity(epoch_sectors.len());
            let mut total_power = PowerPair::zero();
            let mut total_pledge = TokenAmount::zero();

            for sector in epoch_sectors {
                sector_numbers.push(sector.sector_number);
                total_power += &power_for_sector(sector_size, &sector);
                total_pledge += &sector.initial_pledge;
            }

            SectorEpochSet {
                epoch: expiration,
                sectors: sector_numbers,
                power: total_power,
                pledge: total_pledge,
            }
        })
        .collect();

    sector_epoch_sets.sort_by_key(|epoch_set| epoch_set.epoch);
    sector_epoch_sets
}
