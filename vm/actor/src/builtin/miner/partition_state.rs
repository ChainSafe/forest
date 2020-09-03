use super::{
    actor_error, power_for_sectors, ExpirationQueue, ExpirationSet, QuantSpec, SectorOnChainInfo,
    Sectors, TerminationResult,
};
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
    pub expirations_epoch: Cid, // AMT[ChainEpoch]ExpirationSet
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
            expirations_epoch: empty_array_cid.clone(),
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
    ) -> Result<PowerPair, String> {
        let mut expirations = ExpirationQueue::new(store, &self.expirations_epoch, quant)
            .map_err(|e| format!("failed to load sector expirations: {:?}", e))?;

        let (sector_numbers, power, _) = expirations
            .add_active_sectors(sectors, sector_size)
            .map_err(|e| format!("failed to record new sector expirations: {}", e))?;

        self.expirations_epoch = expirations
            .amt
            .flush()
            .map_err(|e| format!("failed to store sector expirations: {:?}", e))?;

        if self.sectors.contains_any(&sector_numbers) {
            return Err("not all added sectors are new".to_string());
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
        let mut queue = ExpirationQueue::new(store, &self.expirations_epoch, quant)
            .map_err(|e| format!("failed to load partition queue: {:?}", e))?;

        // Reschedule faults
        let power = queue
            .reschedule_as_faults(fault_expiration, sectors, sector_size)
            .map_err(|e| format!("failed to add faults to partition queue: {:?}", e))?;

        // Save expiration queue
        self.expirations_epoch = queue.amt.flush()?;

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
        &self,
        store: &BS,
        sectors: Sectors<'_, BS>,
        sector_numbers: BitField,
        fault_expiration_epoch: ChainEpoch,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> (BitField, PowerPair) {
        todo!()

        // 	err = validatePartitionContainsSectors(p, sectorNos)
        // 	if err != nil {
        // 		return BitField{}, NewPowerPairZero(), xc.ErrIllegalArgument.Wrapf("failed fault declaration: %w", err)
        // 	}

        // 	// Split declarations into declarations of new faults, and retraction of declared recoveries.
        // 	retractedRecoveries, err := bitfield.IntersectBitField(p.Recoveries, sectorNos)
        // 	if err != nil {
        // 		return BitField{}, NewPowerPairZero(), xerrors.Errorf("failed to intersect sectors with recoveries: %w", err)
        // 	}

        // 	newFaults, err = bitfield.SubtractBitField(sectorNos, retractedRecoveries)
        // 	if err != nil {
        // 		return BitField{}, NewPowerPairZero(), xerrors.Errorf("failed to subtract recoveries from sectors: %w", err)
        // 	}

        // 	// Ignore any terminated sectors and previously declared or detected faults
        // 	newFaults, err = bitfield.SubtractBitField(newFaults, p.Terminated)
        // 	if err != nil {
        // 		return BitField{}, NewPowerPairZero(), xerrors.Errorf("failed to subtract terminations from faults: %w", err)
        // 	}
        // 	newFaults, err = bitfield.SubtractBitField(newFaults, p.Faults)
        // 	if err != nil {
        // 		return BitField{}, NewPowerPairZero(), xerrors.Errorf("failed to subtract existing faults from faults: %w", err)
        // 	}

        // 	// Add new faults to state.
        // 	newFaultyPower = NewPowerPairZero()
        // 	if newFaultSectors, err := sectors.Load(newFaults); err != nil {
        // 		return BitField{}, NewPowerPairZero(), xerrors.Errorf("failed to load fault sectors: %w", err)
        // 	} else if len(newFaultSectors) > 0 {
        // 		newFaultyPower, err = p.addFaults(store, newFaults, newFaultSectors, faultExpirationEpoch, ssize, quant)
        // 		if err != nil {
        // 			return BitField{}, NewPowerPairZero(), xerrors.Errorf("failed to add faults: %w", err)
        // 		}
        // 	}

        // 	// Remove faulty recoveries from state.
        // 	if retractedRecoverySectors, err := sectors.Load(retractedRecoveries); err != nil {
        // 		return BitField{}, NewPowerPairZero(), xerrors.Errorf("failed to load recovery sectors: %w", err)
        // 	} else if len(retractedRecoverySectors) > 0 {
        // 		retractedRecoveryPower := PowerForSectors(ssize, retractedRecoverySectors)
        // 		err = p.removeRecoveries(retractedRecoveries, retractedRecoveryPower)
        // 		if err != nil {
        // 			return BitField{}, NewPowerPairZero(), xerrors.Errorf("failed to remove recoveries: %w", err)
        // 		}
        // 	}
        // 	return newFaults, newFaultyPower, nil
    }

    /// Removes sector numbers from faults and thus from recoveries.
    /// The sectors are removed from the Faults and Recovering bitfields, and FaultyPower and RecoveringPower reduced.
    /// The sectors are re-scheduled for expiration shortly after their target expiration epoch.
    /// Returns the power of the now-recovered sectors.
    pub fn recover_faults<BS>(
        &self,
        store: &BS,
        sectors: Sectors<'_, BS>,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> PowerPair {
        todo!()

        // 	// Process recoveries, assuming the proof will be successful.
        // 	// This similarly updates state.
        // 	recoveredSectors, err := sectors.Load(p.Recoveries)
        // 	if err != nil {
        // 		return NewPowerPairZero(), xerrors.Errorf("failed to load recovered sectors: %w", err)
        // 	}
        // 	// Load expiration queue
        // 	queue, err := LoadExpirationQueue(store, p.ExpirationsEpochs, quant)
        // 	if err != nil {
        // 		return NewPowerPairZero(), xerrors.Errorf("failed to load partition queue: %w", err)
        // 	}
        // 	// Reschedule recovered
        // 	power, err := queue.RescheduleRecovered(recoveredSectors, ssize)
        // 	if err != nil {
        // 		return NewPowerPairZero(), xerrors.Errorf("failed to reschedule faults in partition queue: %w", err)
        // 	}
        // 	// Save expiration queue
        // 	if p.ExpirationsEpochs, err = queue.Root(); err != nil {
        // 		return NewPowerPairZero(), err
        // 	}

        // 	// Update partition metadata
        // 	if newFaults, err := bitfield.SubtractBitField(p.Faults, p.Recoveries); err != nil {
        // 		return NewPowerPairZero(), err
        // 	} else {
        // 		p.Faults = newFaults
        // 	}
        // 	p.Recoveries = bitfield.New()

        // 	// No change to live power.
        // 	p.FaultyPower = p.FaultyPower.Sub(power)
        // 	p.RecoveringPower = p.RecoveringPower.Sub(power)

        // 	return power, err
    }

    /// Declares sectors as recovering. Non-faulty and already recovering sectors will be skipped.
    pub fn declare_faults_recovered<BS: BlockStore>(
        &self,
        sectors: Sectors<'_, BS>,
        sector_size: SectorSize,
        sector_numbers: BitField,
    ) {
        todo!()

        // 	// Check that the declared sectors are actually assigned to the partition.
        // 	err = validatePartitionContainsSectors(p, sectorNos)
        // 	if err != nil {
        // 		return xc.ErrIllegalArgument.Wrapf("failed fault declaration: %w", err)
        // 	}

        // 	// Ignore sectors not faulty or already declared recovered
        // 	recoveries, err := bitfield.IntersectBitField(sectorNos, p.Faults)
        // 	if err != nil {
        // 		return xerrors.Errorf("failed to intersect recoveries with faults: %w", err)
        // 	}
        // 	recoveries, err = bitfield.SubtractBitField(recoveries, p.Recoveries)
        // 	if err != nil {
        // 		return xerrors.Errorf("failed to subtract existing recoveries: %w", err)
        // 	}

        // 	// Record the new recoveries for processing at Window PoSt or deadline cron.
        // 	recoverySectors, err := sectors.Load(recoveries)
        // 	if err != nil {
        // 		return xerrors.Errorf("failed to load recovery sectors: %w", err)
        // 	}

        // 	p.Recoveries, err = bitfield.MergeBitFields(p.Recoveries, recoveries)
        // 	if err != nil {
        // 		return err
        // 	}

        // 	power := PowerForSectors(ssize, recoverySectors)
        // 	p.RecoveringPower = p.RecoveringPower.Add(power)
        // 	// No change to faults, or terminations.
        // 	// No change to faulty power.
        // 	return nil
    }

    /// Removes sectors from recoveries and recovering power. Assumes sectors are currently faulty and recovering.
    pub fn remove_recoveries(&mut self, sector_numbers: BitField, power: PowerPair) {
        todo!()

        // 	empty, err := sectorNos.IsEmpty()
        // 	if err != nil {
        // 		return err
        // 	}
        // 	if empty {
        // 		return nil
        // 	}
        // 	p.Recoveries, err = bitfield.SubtractBitField(p.Recoveries, sectorNos)
        // 	if err != nil {
        // 		return err
        // 	}
        // 	p.RecoveringPower = p.RecoveringPower.Sub(power)
        // 	// No change to faults, or terminations.
        // 	// No change to faulty power.
        // 	return nil
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
        sectors: Sectors<'_, BS>,
        new_expiration: ChainEpoch,
        sector_numbers: BitField,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> BitField {
        todo!()

        // 	// Ensure these sectors actually belong to this partition.
        // 	present, err := bitfield.IntersectBitField(sectorNos, p.Sectors)
        // 	if err != nil {
        // 		return BitField{}, err
        // 	}

        // 	// Filter out terminated sectors.
        // 	live, err := bitfield.SubtractBitField(present, p.Terminated)
        // 	if err != nil {
        // 		return BitField{}, err
        // 	}

        // 	// Filter out faulty sectors.
        // 	active, err := bitfield.SubtractBitField(live, p.Faults)
        // 	if err != nil {
        // 		return BitField{}, err
        // 	}

        // 	sectorInfos, err := sectors.Load(active)
        // 	if err != nil {
        // 		return BitField{}, err
        // 	}

        // 	expirations, err := LoadExpirationQueue(store, p.ExpirationsEpochs, quant)
        // 	if err != nil {
        // 		return BitField{}, xerrors.Errorf("failed to load sector expirations: %w", err)
        // 	}
        // 	if err = expirations.RescheduleExpirations(newExpiration, sectorInfos, ssize); err != nil {
        // 		return BitField{}, err
        // 	}
        // 	p.ExpirationsEpochs, err = expirations.Root()
        // 	if err != nil {
        // 		return BitField{}, err
        // 	}

        // 	return active, nil
    }

    /// Replaces a number of "old" sectors with new ones.
    /// The old sectors must not be faulty or terminated.
    /// If the same sector is both removed and added, this permits rescheduling *with a change in power*,
    /// unlike RescheduleExpirations.
    /// Returns the delta to power and pledge requirement.
    pub fn replace_sectors<BS: BlockStore>(
        &mut self,
        store: &BS,
        old_sectors: Vec<SectorOnChainInfo>,
        new_sectors: Vec<SectorOnChainInfo>,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> (PowerPair, TokenAmount) {
        todo!()

        // 	expirations, err := LoadExpirationQueue(store, p.ExpirationsEpochs, quant)
        // 	if err != nil {
        // 		return NewPowerPairZero(), big.Zero(), xerrors.Errorf("failed to load sector expirations: %w", err)
        // 	}
        // 	oldSnos, newSnos, powerDelta, pledgeDelta, err := expirations.ReplaceSectors(oldSectors, newSectors, ssize)
        // 	if err != nil {
        // 		return NewPowerPairZero(), big.Zero(), xerrors.Errorf("failed to replace sector expirations: %w", err)
        // 	}
        // 	if p.ExpirationsEpochs, err = expirations.Root(); err != nil {
        // 		return NewPowerPairZero(), big.Zero(), xerrors.Errorf("failed to save sector expirations: %w", err)
        // 	}

        // 	// Check the sectors being removed are active (alive, not faulty).
        // 	active, err := p.ActiveSectors()
        // 	if err != nil {
        // 		return NewPowerPairZero(), big.Zero(), err
        // 	}
        // 	allActive, err := abi.BitFieldContainsAll(active, oldSnos)
        // 	if err != nil {
        // 		return NewPowerPairZero(), big.Zero(), xerrors.Errorf("failed to check for active sectors: %w", err)
        // 	} else if !allActive {
        // 		return NewPowerPairZero(), big.Zero(), xerrors.Errorf("refusing to replace inactive sectors in %v (active: %v)", oldSnos, active)
        // 	}

        // 	// Update partition metadata.
        // 	if p.Sectors, err = bitfield.SubtractBitField(p.Sectors, oldSnos); err != nil {
        // 		return NewPowerPairZero(), big.Zero(), xerrors.Errorf("failed to remove replaced sectors: %w", err)
        // 	}
        // 	if p.Sectors, err = bitfield.MergeBitFields(p.Sectors, newSnos); err != nil {
        // 		return NewPowerPairZero(), big.Zero(), xerrors.Errorf("failed to add replaced sectors: %w", err)
        // 	}
        // 	p.LivePower = p.LivePower.Add(powerDelta)
        // 	// No change to faults, recoveries, or terminations.
        // 	// No change to faulty or recovering power.
        // 	return powerDelta, pledgeDelta, nil
    }

    /// Record the epoch of any sectors expiring early, for termination fee calculation later.
    pub fn record_early_termination<BS: BlockStore>(
        &self,
        store: &BS,
        epoch: ChainEpoch,
        sectors: BitField,
    ) {
        todo!()

        // 	etQueue, err := LoadBitfieldQueue(store, p.EarlyTerminated, NoQuantization)
        // 	if err != nil {
        // 		return xerrors.Errorf("failed to load early termination queue: %w", err)
        // 	}
        // 	if err = etQueue.AddToQueue(epoch, sectors); err != nil {
        // 		return xerrors.Errorf("failed to add to early termination queue: %w", err)
        // 	}
        // 	if p.EarlyTerminated, err = etQueue.Root(); err != nil {
        // 		return xerrors.Errorf("failed to save early termination queue: %w", err)
        // 	}
        // 	return nil
    }

    /// Marks a collection of sectors as terminated.
    /// The sectors are removed from Faults and Recoveries.
    /// The epoch of termination is recorded for future termination fee calculation.
    pub fn terminate_sectors<BS: BlockStore>(
        &mut self,
        store: &BS,
        sectors: Sectors<'_, BS>,
        epoch: ChainEpoch,
        sector_numbers: BitField,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> ExpirationSet {
        todo!()

        // 	liveSectors, err := p.LiveSectors()
        // 	if err != nil {
        // 		return nil, err
        // 	}
        // 	if contains, err := abi.BitFieldContainsAll(liveSectors, sectorNos); err != nil {
        // 		return nil, xc.ErrIllegalArgument.Wrapf("failed to intersect live sectors with terminating sectors: %w", err)
        // 	} else if !contains {
        // 		return nil, xc.ErrIllegalArgument.Wrapf("can only terminate live sectors")
        // 	}

        // 	sectorInfos, err := sectors.Load(sectorNos)
        // 	if err != nil {
        // 		return nil, err
        // 	}
        // 	expirations, err := LoadExpirationQueue(store, p.ExpirationsEpochs, quant)
        // 	if err != nil {
        // 		return nil, xerrors.Errorf("failed to load sector expirations: %w", err)
        // 	}
        // 	removed, removedRecovering, err := expirations.RemoveSectors(sectorInfos, p.Faults, p.Recoveries, ssize)
        // 	if err != nil {
        // 		return nil, xerrors.Errorf("failed to remove sector expirations: %w", err)
        // 	}
        // 	if p.ExpirationsEpochs, err = expirations.Root(); err != nil {
        // 		return nil, xerrors.Errorf("failed to save sector expirations: %w", err)
        // 	}

        // 	removedSectors, err := bitfield.MergeBitFields(removed.OnTimeSectors, removed.EarlySectors)
        // 	if err != nil {
        // 		return nil, err
        // 	}

        // 	// Record early termination.
        // 	err = p.recordEarlyTermination(store, epoch, removedSectors)
        // 	if err != nil {
        // 		return nil, xerrors.Errorf("failed to record early sector termination: %w", err)
        // 	}

        // 	// Update partition metadata.
        // 	if p.Faults, err = bitfield.SubtractBitField(p.Faults, removedSectors); err != nil {
        // 		return nil, xerrors.Errorf("failed to remove terminated sectors from faults: %w", err)
        // 	}
        // 	if p.Recoveries, err = bitfield.SubtractBitField(p.Recoveries, removedSectors); err != nil {
        // 		return nil, xerrors.Errorf("failed to remove terminated sectors from recoveries: %w", err)
        // 	}
        // 	if p.Terminated, err = bitfield.MergeBitFields(p.Terminated, removedSectors); err != nil {
        // 		return nil, xerrors.Errorf("failed to add terminated sectors: %w", err)
        // 	}

        // 	p.LivePower = p.LivePower.Sub(removed.ActivePower).Sub(removed.FaultyPower)
        // 	p.FaultyPower = p.FaultyPower.Sub(removed.FaultyPower)
        // 	p.RecoveringPower = p.RecoveringPower.Sub(removedRecovering)

        // 	return removed, nil
    }

    /// PopExpiredSectors traverses the expiration queue up to and including some epoch, and marks all expiring
    /// sectors as terminated.
    /// Returns the expired sector aggregates.
    pub fn pop_expired_sectors<BS: BlockStore>(
        &self,
        store: &BS,
        until: ChainEpoch,
        quant: QuantSpec,
    ) -> ExpirationSet {
        todo!()

        // 	expirations, err := LoadExpirationQueue(store, p.ExpirationsEpochs, quant)
        // 	if err != nil {
        // 		return nil, xerrors.Errorf("failed to load expiration queue: %w", err)
        // 	}
        // 	popped, err := expirations.PopUntil(until)
        // 	if err != nil {
        // 		return nil, xerrors.Errorf("failed to pop expiration queue until %d: %w", until, err)
        // 	}
        // 	if p.ExpirationsEpochs, err = expirations.Root(); err != nil {
        // 		return nil, err
        // 	}

        // 	expiredSectors, err := bitfield.MergeBitFields(popped.OnTimeSectors, popped.EarlySectors)
        // 	if err != nil {
        // 		return nil, err
        // 	}

        // 	// There shouldn't be any recovering sectors or power if this is invoked at deadline end.
        // 	// Either the partition was PoSted and the recovering became recovered, or the partition was not PoSted
        // 	// and all recoveries retracted.
        // 	// No recoveries may be posted until the deadline is closed.
        // 	noRecoveries, err := p.Recoveries.IsEmpty()
        // 	if err != nil {
        // 		return nil, err
        // 	} else if !noRecoveries {
        // 		return nil, xerrors.Errorf("unexpected recoveries while processing expirations")
        // 	}
        // 	if !p.RecoveringPower.IsZero() {
        // 		return nil, xerrors.Errorf("unexpected recovering power while processing expirations")
        // 	}
        // 	// Nothing expiring now should have already terminated.
        // 	alreadyTerminated, err := abi.BitFieldContainsAny(p.Terminated, expiredSectors)
        // 	if err != nil {
        // 		return nil, err
        // 	} else if alreadyTerminated {
        // 		return nil, xerrors.Errorf("expiring sectors already terminated")
        // 	}

        // 	// Mark the sectors as terminated and subtract sector power.
        // 	if p.Terminated, err = bitfield.MergeBitFields(p.Terminated, expiredSectors); err != nil {
        // 		return nil, xerrors.Errorf("failed to merge expired sectors: %w", err)
        // 	}
        // 	if p.Faults, err = bitfield.SubtractBitField(p.Faults, expiredSectors); err != nil {
        // 		return nil, err
        // 	}
        // 	p.LivePower = p.LivePower.Sub(popped.ActivePower.Add(popped.FaultyPower))
        // 	p.FaultyPower = p.FaultyPower.Sub(popped.FaultyPower)

        // 	// Record the epoch of any sectors expiring early, for termination fee calculation later.
        // 	err = p.recordEarlyTermination(store, until, popped.EarlySectors)
        // 	if err != nil {
        // 		return nil, xerrors.Errorf("failed to record early terminations: %w", err)
        // 	}

        // 	return popped, nil
    }

    /// Marks all non-faulty sectors in the partition as faulty and clears recoveries, updating power memos appropriately.
    /// All sectors' expirations are rescheduled to the fault expiration, as "early" (if not expiring earlier)
    /// Returns the power of the newly faulty and failed recovery sectors.
    pub fn record_missed_post<BS: BlockStore>(
        &mut self,
        store: &BS,
        fault_expiration: ChainEpoch,
        quant: QuantSpec,
    ) -> (PowerPair, PowerPair) {
        todo!()

        // 	// Collapse tail of queue into the last entry, and mark all power faulty.
        // 	// Load expiration queue
        // 	queue, err := LoadExpirationQueue(store, p.ExpirationsEpochs, quant)
        // 	if err != nil {
        // 		return NewPowerPairZero(), NewPowerPairZero(), xerrors.Errorf("failed to load partition queue: %w", err)
        // 	}
        // 	if err = queue.RescheduleAllAsFaults(faultExpiration); err != nil {
        // 		return NewPowerPairZero(), NewPowerPairZero(), xerrors.Errorf("failed to reschedule all as faults: %w", err)
        // 	}
        // 	// Save expiration queue
        // 	if p.ExpirationsEpochs, err = queue.Root(); err != nil {
        // 		return NewPowerPairZero(), NewPowerPairZero(), err
        // 	}

        // 	// Compute faulty power for penalization. New faulty power is the total power minus already faulty.
        // 	newFaultPower = p.LivePower.Sub(p.FaultyPower)
        // 	failedRecoveryPower = p.RecoveringPower

        // 	// Update partition metadata
        // 	allFaults, err := p.LiveSectors()
        // 	if err != nil {
        // 		return NewPowerPairZero(), NewPowerPairZero(), err
        // 	}
        // 	p.Faults = allFaults
        // 	p.Recoveries = bitfield.New()
        // 	p.FaultyPower = p.LivePower
        // 	p.RecoveringPower = NewPowerPairZero()

        // 	return newFaultPower, failedRecoveryPower, nil
    }

    pub fn pop_early_terminations<BS: BlockStore>(
        &mut self,
        store: &BS,
        max_sectors: u64,
    ) -> (TerminationResult, bool) {
        todo!()

        // 	stopErr := errors.New("stop iter")

        // 	// Load early terminations.
        // 	earlyTerminatedQ, err := LoadBitfieldQueue(store, p.EarlyTerminated, NoQuantization)
        // 	if err != nil {
        // 		return TerminationResult{}, false, err
        // 	}

        // 	var (
        // 		processed        []uint64
        // 		hasRemaining     bool
        // 		remainingSectors BitField
        // 		remainingEpoch   abi.ChainEpoch
        // 	)

        // 	result.PartitionsProcessed = 1
        // 	result.Sectors = make(map[abi.ChainEpoch]BitField)

        // 	if err = earlyTerminatedQ.ForEach(func(epoch abi.ChainEpoch, sectors BitField) error {
        // 		toProcess := sectors
        // 		count, err := sectors.Count()
        // 		if err != nil {
        // 			return xerrors.Errorf("failed to count early terminations: %w", err)
        // 		}

        // 		limit := maxSectors - result.SectorsProcessed

        // 		if limit < count {
        // 			toProcess, err = sectors.Slice(0, limit)
        // 			if err != nil {
        // 				return xerrors.Errorf("failed to slice early terminations: %w", err)
        // 			}

        // 			rest, err := bitfield.SubtractBitField(sectors, toProcess)
        // 			if err != nil {
        // 				return xerrors.Errorf("failed to subtract processed early terminations: %w", err)
        // 			}
        // 			hasRemaining = true
        // 			remainingSectors = rest
        // 			remainingEpoch = epoch

        // 			result.SectorsProcessed += limit
        // 		} else {
        // 			processed = append(processed, uint64(epoch))
        // 			result.SectorsProcessed += count
        // 		}

        // 		result.Sectors[epoch] = toProcess

        // 		if result.SectorsProcessed < maxSectors {
        // 			return nil
        // 		}
        // 		return stopErr
        // 	}); err != nil && err != stopErr {
        // 		return TerminationResult{}, false, xerrors.Errorf("failed to walk early terminations queue: %w", err)
        // 	}

        // 	// Update early terminations
        // 	err = earlyTerminatedQ.BatchDelete(processed)
        // 	if err != nil {
        // 		return TerminationResult{}, false, xerrors.Errorf("failed to remove entries from early terminations queue: %w", err)
        // 	}

        // 	if hasRemaining {
        // 		err = earlyTerminatedQ.Set(uint64(remainingEpoch), remainingSectors)
        // 		if err != nil {
        // 			return TerminationResult{}, false, xerrors.Errorf("failed to update remaining entry early terminations queue: %w", err)
        // 		}
        // 	}

        // 	// Save early terminations.
        // 	p.EarlyTerminated, err = earlyTerminatedQ.Root()
        // 	if err != nil {
        // 		return TerminationResult{}, false, xerrors.Errorf("failed to store early terminations queue: %w", err)
        // 	}
        // 	return result, earlyTerminatedQ.Length() > 0, nil
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
        sectors: Sectors<'_, BS>,
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
            .load(&retracted_recoveries)
            .map_err(|e| actor_error!(ErrIllegalState; "failed to load sectors: {:?}", e))?;
        let retracted_recovery_power = power_for_sectors(sector_size, &retracted_recovery_sectors);

        // Ignore skipped faults that are already faults or terminated.
        let new_faults = &(skipped - &self.terminated) - &self.faults;
        let new_fault_sectors = sectors
            .load(&new_faults)
            .map_err(|e| actor_error!(ErrIllegalState; "failed to load sectors: {:?}", e))?;

        // Record new faults
        let new_fault_power = self.add_faults(
            store,
            &new_faults,
            &new_fault_sectors,
            fault_expiration,
            sector_size,
            quant,
        );

        todo!()

        // 	retractedRecoveryPower = PowerForSectors(ssize, retractedRecoverySectors)

        // 	// Ignore skipped faults that are already faults or terminated.
        // 	newFaults, err := bitfield.SubtractBitField(skipped, p.Terminated)
        // 	if err != nil {
        // 		return NewPowerPairZero(), NewPowerPairZero(), xc.ErrIllegalState.Wrapf("failed to subtract terminations from skipped: %w", err)
        // 	}
        // 	newFaults, err = bitfield.SubtractBitField(newFaults, p.Faults)
        // 	if err != nil {
        // 		return NewPowerPairZero(), NewPowerPairZero(), xc.ErrIllegalState.Wrapf("failed to subtract existing faults from skipped: %w", err)
        // 	}
        // 	newFaultSectors, err := sectors.Load(newFaults)
        // 	if err != nil {
        // 		return NewPowerPairZero(), NewPowerPairZero(), xc.ErrIllegalState.Wrapf("failed to load sectors: %w", err)
        // 	}

        // 	// Record new faults
        // 	newFaultPower, err = p.addFaults(store, newFaults, newFaultSectors, faultExpiration, ssize, quant)
        // 	if err != nil {
        // 		return NewPowerPairZero(), NewPowerPairZero(), xc.ErrIllegalState.Wrapf("failed to add skipped faults: %w", err)
        // 	}

        // 	// Remove faulty recoveries
        // 	err = p.removeRecoveries(retractedRecoveries, retractedRecoveryPower)
        // 	if err != nil {
        // 		return NewPowerPairZero(), NewPowerPairZero(), xc.ErrIllegalState.Wrapf("failed to remove recoveries: %w", err)
        // 	}

        // 	return newFaultPower, retractedRecoveryPower, nil
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

impl ops::Neg for &PowerPair {
    type Output = PowerPair;

    fn neg(self) -> Self::Output {
        PowerPair {
            raw: -&self.raw,
            qa: -&self.qa,
        }
    }
}
