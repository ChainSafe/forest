// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{
    BitFieldQueue, ExpirationSet, Partition, PartitionSectorMap, PoStPartition, PowerPair,
    QuantSpec, SectorOnChainInfo, Sectors, TerminationResult, WPOST_PERIOD_DEADLINES,
};
use crate::{actor_error, ActorError, ExitCode, TokenAmount};
use bitfield::BitField;
use cid::{multihash::Blake2b256, Cid};
use clock::ChainEpoch;
use encoding::tuple::*;
use fil_types::SectorSize;
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use num_traits::Zero;
use std::{cmp, collections::HashMap, collections::HashSet, error::Error as StdError};

/// Deadlines contains Deadline objects, describing the sectors due at the given
/// deadline and their state (faulty, terminated, recovering, etc.).
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct Deadlines {
    // Note: we could inline part of the deadline struct (e.g., active/assigned sectors)
    // to make new sector assignment cheaper. At the moment, assigning a sector requires
    // loading all deadlines to figure out where best to assign new sectors.
    // TODO: change this to an array once the `LengthAtMost32` trait is no more
    pub due: Vec<Cid>, // []Deadline
}

impl Deadlines {
    pub fn new(empty_deadline_cid: Cid) -> Self {
        Self {
            due: vec![empty_deadline_cid; WPOST_PERIOD_DEADLINES as usize],
        }
    }

    pub fn load_deadline<BS: BlockStore>(
        &self,
        store: &BS,
        deadline_idx: u64,
    ) -> Result<Deadline, ActorError> {
        if deadline_idx >= WPOST_PERIOD_DEADLINES as u64 {
            return Err(actor_error!(
                ErrIllegalArgument,
                "invalid deadline {}",
                deadline_idx
            ));
        }

        store
            .get(&self.due[deadline_idx as usize])
            .ok()
            .flatten()
            .ok_or_else(|| {
                actor_error!(
                    ErrIllegalState,
                    "failed to lookup deadline {}",
                    deadline_idx
                )
            })
    }

    pub fn for_each<BS: BlockStore>(
        &self,
        store: &BS,
        mut f: impl FnMut(u64, Deadline) -> Result<(), Box<dyn StdError>>,
    ) -> Result<(), Box<dyn StdError>> {
        for i in 0..self.due.len() {
            let index = i as u64;
            let deadline = self.load_deadline(store, index)?;
            f(index, deadline)?;
        }
        Ok(())
    }

    pub fn update_deadline<BS: BlockStore>(
        &mut self,
        store: &BS,
        deadline_idx: u64,
        deadline: &Deadline,
    ) -> Result<(), Box<dyn StdError>> {
        if deadline_idx >= WPOST_PERIOD_DEADLINES as u64 {
            return Err(format!("invalid deadline {}", deadline_idx).into());
        }
        self.due[deadline_idx as usize] = store.put(deadline, Blake2b256)?;
        Ok(())
    }
}

/// Deadline holds the state for all sectors due at a specific deadline.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct Deadline {
    /// Partitions in this deadline, in order.
    /// The keys of this AMT are always sequential integers beginning with zero.
    pub partitions: Cid, // AMT[PartitionNumber]Partition

    /// Maps epochs to partitions that _may_ have sectors that expire in or
    /// before that epoch, either on-time or early as faults.
    /// Keys are quantized to final epochs in each proving deadline.
    ///
    /// NOTE: Partitions MUST NOT be removed from this queue (until the
    /// associated epoch has passed) even if they no longer have sectors
    /// expiring at that epoch. Sectors expiring at this epoch may later be
    /// recovered, and this queue will not be updated at that time.
    pub expirations_epochs: Cid, // AMT[ChainEpoch]BitField

    /// Partitions numbers with PoSt submissions since the proving period started.
    pub post_submissions: BitField,

    /// Partitions with sectors that terminated early.
    pub early_terminations: BitField,

    /// The number of non-terminated sectors in this deadline (incl faulty).
    pub live_sectors: u64,

    /// The total number of sectors in this deadline (incl dead).
    pub total_sectors: u64,

    /// Memoized sum of faulty power in partitions.
    pub faulty_power: PowerPair,
}

impl Deadline {
    pub fn new(empty_array_cid: Cid) -> Self {
        Self {
            partitions: empty_array_cid.clone(),
            expirations_epochs: empty_array_cid,
            post_submissions: BitField::new(),
            early_terminations: BitField::new(),
            live_sectors: 0,
            total_sectors: 0,
            faulty_power: PowerPair::zero(),
        }
    }

    pub fn partitions_amt<'db, BS: BlockStore>(
        &self,
        store: &'db BS,
    ) -> Result<Amt<'db, Partition, BS>, ActorError> {
        Amt::load(&self.partitions, store)
            .map_err(|e| actor_error!(ErrIllegalState; "failed to load partitions: {:?}", e))
    }

    pub fn load_partition<BS: BlockStore>(
        &self,
        store: &BS,
        partition_idx: u64,
    ) -> Result<Partition, String> {
        let partitions = Amt::<Partition, _>::load(&self.partitions, store)
            .map_err(|e| format!("failed to load partitions: {:?}", e))?;

        let partition = partitions
            .get(partition_idx)
            .map_err(|e| format!("failed to lookup partition {}: {:?}", partition_idx, e))?;

        partition
            .cloned()
            .ok_or_else(|| format!("no partition {}", partition_idx))
    }

    /// Adds some partition numbers to the set expiring at an epoch.
    pub fn add_expiration_partitions<BS: BlockStore>(
        &mut self,
        store: &BS,
        expiration_epoch: ChainEpoch,
        partitions: &[u64],
        quant: QuantSpec,
    ) -> Result<(), String> {
        // Avoid doing any work if there's nothing to reschedule.
        if partitions.is_empty() {
            return Ok(());
        }

        let mut queue = BitFieldQueue::new(store, &self.expirations_epochs, quant)
            .map_err(|e| format!("failed to load expiration queue: {:?}", e))?;
        queue
            .add_to_queue_values(expiration_epoch, partitions)
            .map_err(|e| format!("failed to mutate expiration queue: {}", e))?;
        self.expirations_epochs = queue
            .amt
            .flush()
            .map_err(|e| format!("failed to save expiration queue: {:?}", e))?;

        Ok(())
    }

    /// PopExpiredSectors terminates expired sectors from all partitions.
    /// Returns the expired sector aggregates.
    pub fn pop_expired_sectors<BS: BlockStore>(
        &mut self,
        store: &BS,
        until: ChainEpoch,
        quant: QuantSpec,
    ) -> Result<ExpirationSet, Box<dyn StdError>> {
        let (expired_partitions, modified) = self.pop_expired_partitions(store, until, quant)?;

        if !modified {
            // nothing to do.
            return Ok(ExpirationSet::empty());
        }

        let mut partitions = self.partitions_amt(store)?;

        let mut on_time_sectors = Vec::<BitField>::new();
        let mut early_sectors = Vec::<BitField>::new();
        let mut all_on_time_pledge = TokenAmount::zero();
        let mut all_active_power = PowerPair::zero();
        let mut all_faulty_power = PowerPair::zero();
        let mut partitions_with_early_terminations = Vec::<u64>::new();

        // For each partition with an expiry, remove and collect expirations from the partition queue.
        for i in expired_partitions.iter() {
            let partition_idx = i as u64;
            let mut partition = partitions
                .get(partition_idx)?
                .cloned()
                .ok_or_else(|| format!("missing expected partition {}", partition_idx))?;

            let partition_expiration =
                partition
                    .pop_expired_sectors(store, until, quant)
                    .map_err(|e| {
                        ActorError::downcast_wrap(
                            e,
                            format!(
                                "failed to pop expired sectors from partition {}",
                                partition_idx
                            ),
                        )
                    })?;

            if !partition_expiration.early_sectors.is_empty() {
                partitions_with_early_terminations.push(partition_idx);
            }

            on_time_sectors.push(partition_expiration.on_time_sectors);
            early_sectors.push(partition_expiration.early_sectors);
            all_active_power += &partition_expiration.active_power;
            all_faulty_power += &partition_expiration.faulty_power;
            all_on_time_pledge += &partition_expiration.on_time_pledge;

            partitions.set(partition_idx, partition)?;
        }

        self.partitions = partitions.flush()?;

        // Update early expiration bitmap.
        for partition_idx in partitions_with_early_terminations {
            self.early_terminations.set(partition_idx as usize);
        }

        let all_on_time_sectors = BitField::union(&on_time_sectors);
        let all_early_sectors = BitField::union(&early_sectors);

        // Update live sector count.
        let on_time_count = all_on_time_sectors.len();
        let early_count = all_early_sectors.len();
        self.live_sectors -= (on_time_count + early_count) as u64;

        self.faulty_power -= &all_faulty_power;

        Ok(ExpirationSet {
            on_time_sectors: all_on_time_sectors,
            early_sectors: all_early_sectors,
            on_time_pledge: all_on_time_pledge,
            active_power: all_active_power,
            faulty_power: all_faulty_power,
        })
    }

    /// Adds sectors to a deadline. It's the caller's responsibility to make sure
    /// that this deadline isn't currently "open" (i.e., being proved at this point
    /// in time).
    /// The sectors are assumed to be non-faulty.
    pub fn add_sectors<BS: BlockStore>(
        &mut self,
        store: &BS,
        partition_size: u64,
        mut sectors: &[SectorOnChainInfo],
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> Result<PowerPair, Box<dyn StdError>> {
        if sectors.is_empty() {
            return Ok(PowerPair::zero());
        }

        // First update partitions, consuming the sectors
        let mut partition_deadline_updates = HashMap::<ChainEpoch, Vec<u64>>::new();
        let mut new_power = PowerPair::zero();
        self.live_sectors += sectors.len() as u64;
        self.total_sectors += sectors.len() as u64;

        let mut partitions = self.partitions_amt(store)?;

        // try filling up the last partition first.
        for partition_idx in partitions.count().saturating_sub(1).. {
            if sectors.is_empty() {
                break;
            }

            // Get/create partition to update.
            let mut partition = match partitions.get(partition_idx)? {
                Some(partition) => partition.clone(),
                None => {
                    // This case will usually happen zero times.
                    // It would require adding more than a full partition in one go
                    // to happen more than once.
                    Partition::new(Amt::<Cid, BS>::new(store).flush()?)
                }
            };

            // Figure out which (if any) sectors we want to add to this partition.
            let sector_count = partition.sectors.len() as u64;
            if sector_count >= partition_size {
                continue;
            }

            let size = cmp::min(partition_size - sector_count, sectors.len() as u64);
            let (start, partition_new_sectors) = sectors.split_at(size as usize);
            sectors = start;

            // Add sectors to partition.
            let partition_new_power =
                partition.add_sectors(store, partition_new_sectors, sector_size, quant)?;
            new_power += &partition_new_power;

            // Save partition back.
            partitions.set(partition_idx, partition)?;

            // Record deadline -> partition mapping so we can later update the deadlines.
            for sector in partition_new_sectors {
                if let Some(partition_update) =
                    partition_deadline_updates.get_mut(&sector.expiration)
                {
                    if partition_update.last() != Some(&partition_idx) {
                        partition_update.push(partition_idx);
                    }
                }
            }
        }

        // Save partitions back.
        self.partitions = partitions.flush()?;

        // Next, update the expiration queue.
        let mut deadline_expirations =
            BitFieldQueue::new(store, &self.expirations_epochs, quant)
                .map_err(|e| format!("failed to load expiration epochs: {:?}", e))?;
        deadline_expirations
            .add_many_to_queue_values(&partition_deadline_updates)
            .map_err(|e| {
                ActorError::downcast_wrap(e, "failed to add expirations for new deadlines")
            })?;
        self.expirations_epochs = deadline_expirations.amt.flush()?;

        Ok(new_power)
    }

    pub fn pop_early_terminations<BS: BlockStore>(
        &mut self,
        store: &BS,
        max_partitions: u64,
        max_sectors: u64,
    ) -> Result<(TerminationResult, /* has more */ bool), Box<dyn StdError>> {
        let mut partitions = self.partitions_amt(store)?;

        let mut partitions_finished = Vec::<u64>::new();
        let mut result = TerminationResult::new();

        for i in self.early_terminations.iter() {
            let partition_idx = i as u64;

            let mut partition = match partitions
                .get(partition_idx)
                .map_err(|e| format!("failed to load partition {}: {:?}", partition_idx, e))?
            {
                Some(partition) => partition.clone(),
                None => {
                    partitions_finished.push(partition_idx);
                    continue;
                }
            };

            // Pop early terminations.
            let (partition_result, more) = partition
                .pop_early_terminations(store, max_sectors - result.sectors_processed)
                .map_err(|e| {
                    ActorError::downcast_wrap(e, "failed to pop terminations from partition")
                })?;

            result += partition_result;

            // If we've processed all of them for this partition, unmark it in the deadline.
            if !more {
                partitions_finished.push(partition_idx);
            }

            // Save partition
            partitions
                .set(partition_idx, partition)
                .map_err(|e| format!("failed to store partition {}: {:?}", partition_idx, e))?;

            if !result.below_limit(max_partitions, max_sectors) {
                break;
            }
        }

        // Removed finished partitions from the index.
        for finished in partitions_finished {
            self.early_terminations.unset(finished as usize);
        }

        // Save deadline's partitions
        self.partitions = partitions
            .flush()
            .map_err(|e| format!("failed to update partitions: {:?}", e))?;

        // Update global early terminations bitfield.
        let no_early_terminations = self.early_terminations.is_empty();
        Ok((result, !no_early_terminations))
    }

    pub fn pop_expired_partitions<BS: BlockStore>(
        &mut self,
        store: &BS,
        until: ChainEpoch,
        quant: QuantSpec,
    ) -> Result<(BitField, bool), Box<dyn StdError>> {
        let mut expirations = BitFieldQueue::new(store, &self.expirations_epochs, quant)?;
        let (popped, modified) = expirations
            .pop_until(until)
            .map_err(|e| ActorError::downcast_wrap(e, "failed to pop expiring partitions"))?;

        if modified {
            self.expirations_epochs = expirations.amt.flush()?;
        }

        Ok((popped, modified))
    }

    pub fn terminate_sectors<BS: BlockStore>(
        &mut self,
        store: &BS,
        sectors: &Sectors<'_, BS>,
        epoch: ChainEpoch,
        partition_sectors: &PartitionSectorMap,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> Result<PowerPair, Box<dyn StdError>> {
        let mut partitions = self.partitions_amt(store)?;

        let mut power_lost = PowerPair::zero();
        for (partition_idx, sector_numbers) in partition_sectors.iter() {
            let mut partition = partitions
                .get(partition_idx)
                .map_err(|e| format!("failed to load partition {}: {:?}", partition_idx, e))?
                .ok_or_else(
                    || actor_error!(ErrNotFound; "failed to find partition {}", partition_idx),
                )?
                .clone();

            let removed = partition
                .terminate_sectors(store, sectors, epoch, sector_numbers, sector_size, quant)
                .map_err(|e| {
                    ActorError::downcast_wrap(
                        e,
                        format!("failed to terminate sectors in partition {}", partition_idx),
                    )
                })?;

            partitions.set(partition_idx, partition).map_err(|e| {
                format!(
                    "failed to store updated partition {}: {:?}",
                    partition_idx, e
                )
            })?;

            if !removed.is_empty() {
                // Record that partition now has pending early terminations.
                self.early_terminations.set(partition_idx as usize);

                // Record change to sectors and power
                self.live_sectors -= removed.len() as u64;
            } // note: we should _always_ have early terminations, unless the early termination bitfield is empty.

            self.faulty_power -= &removed.faulty_power;

            // Aggregate power lost from active sectors
            power_lost += &removed.active_power;
        }

        // save partitions back
        self.partitions = partitions
            .flush()
            .map_err(|e| format!("failed to persist partitions: {:?}", e))?;

        Ok(power_lost)
    }

    /// RemovePartitions removes the specified partitions, shifting the remaining
    /// ones to the left, and returning the live and dead sectors they contained.
    ///
    /// Returns an error if any of the partitions contained faulty sectors or early
    /// terminations.
    pub fn remove_partitions<BS: BlockStore>(
        &mut self,
        store: &BS,
        to_remove: &BitField,
        quant: QuantSpec,
    ) -> Result<
        (
            BitField,  // live
            BitField,  // dead
            PowerPair, // removed power
        ),
        Box<dyn StdError>,
    > {
        let old_partitions = self
            .partitions_amt(store)
            .map_err(|e| e.wrap("failed to load partitions"))?;

        let partition_count = old_partitions.count();
        let to_remove_set: HashSet<_> = to_remove
            .bounded_iter(partition_count as usize)
            .map_err(
                |e| actor_error!(ErrIllegalArgument; "failed to expand partitions into map: {}", e),
            )?
            .collect();

        if to_remove_set.is_empty() {
            // Nothing to do.
            return Ok((BitField::new(), BitField::new(), PowerPair::zero()));
        }

        if let Some(partition_idx) = to_remove_set.iter().find(|&&i| i as u64 >= partition_count) {
            return Err(
                actor_error!(ErrIllegalArgument; "partition index {} out of range [0, {})", partition_idx, partition_count).into()
            );
        }

        // Should already be checked earlier, but we might as well check again.
        if !self.early_terminations.is_empty() {
            return Err("cannot remove partitions from deadline with early terminations".into());
        }

        let mut new_partitions = Amt::<Partition, BS>::new(store);
        let mut all_dead_sectors = Vec::<BitField>::with_capacity(to_remove_set.len());
        let mut all_live_sectors = Vec::<BitField>::with_capacity(to_remove_set.len());
        let mut removed_power = PowerPair::zero();

        // TODO: maybe only unmarshal the partition if `to_remove_set` contains the
        // corresponding index, like the Go impl does

        old_partitions
            .for_each(|partition_idx, partition| {
                // If we're keeping the partition as-is, append it to the new partitions array.
                if !to_remove_set.contains(&(partition_idx as usize)) {
                    new_partitions.set(new_partitions.count(), partition.clone())?;
                    return Ok(());
                }

                // Don't allow removing partitions with faulty sectors.
                let has_no_faults = partition.faults.is_empty();
                if !has_no_faults {
                    return Err(actor_error!(ErrIllegalArgument; "cannot remove partition {}: has faults", partition_idx).into());
                }

                // Get the live sectors.
                let live_sectors = partition.live_sectors();

                all_dead_sectors.push(partition.terminated.clone());
                all_live_sectors.push(live_sectors);
                removed_power += &partition.live_power;

                Ok(())
            })
            .map_err(|e| ActorError::downcast_wrap(e, "while removing partitions"))?;

        self.partitions = new_partitions
            .flush()
            .map_err(|e| format!("failed to persist new partition table: {:?}", e))?;

        let dead = BitField::union(&all_dead_sectors);
        let live = BitField::union(&all_live_sectors);

        // Update sector counts.
        let removed_dead_sectors = dead.len() as u64;
        let removed_live_sectors = live.len() as u64;

        self.live_sectors -= removed_live_sectors;
        self.total_sectors -= removed_live_sectors + removed_dead_sectors;

        // Update expiration bitfields.
        let mut expiration_epochs = BitFieldQueue::new(store, &self.expirations_epochs, quant)
            .map_err(|e| format!("failed to load expiration queue: {:?}", e))?;

        expiration_epochs.cut(to_remove).map_err(|e| {
            format!(
                "failed cut removed partitions from deadline expiration queue: {}",
                e
            )
        })?;

        self.expirations_epochs = expiration_epochs
            .amt
            .flush()
            .map_err(|e| format!("failed persist deadline expiration queue: {:?}", e))?;

        Ok((live, dead, removed_power))
    }

    pub fn declare_faults<BS: BlockStore>(
        &mut self,
        store: &BS,
        sectors: &Sectors<'_, BS>,
        sector_size: SectorSize,
        quant: QuantSpec,
        fault_expiration_epoch: ChainEpoch,
        partition_sectors: &PartitionSectorMap,
    ) -> Result<PowerPair, Box<dyn StdError>> {
        let mut partitions = self.partitions_amt(store)?;

        // Record partitions with some fault, for subsequently indexing in the deadline.
        // Duplicate entries don't matter, they'll be stored in a bitfield (a set).
        let mut partitions_with_fault = Vec::<u64>::with_capacity(partition_sectors.len());
        let mut new_faulty_power = PowerPair::zero();

        for (partition_idx, sector_numbers) in partition_sectors.iter() {
            let mut partition = partitions
                .get(partition_idx)
                .map_err(|e| actor_error!(ErrIllegalState; "failed to load partition {}: {:?}", partition_idx, e))?
                .ok_or_else(|| actor_error!(ErrNotFound,
                    "no such partition {}", partition_idx))?.clone();

            let (new_faults, new_partition_faulty_power) = partition
                .declare_faults(
                    store,
                    sectors,
                    sector_numbers,
                    fault_expiration_epoch,
                    sector_size,
                    quant,
                )
                .map_err(|e| {
                    ActorError::downcast_wrap(
                        e,
                        format!("failed to declare faults in partition {}", partition_idx),
                    )
                })?;

            new_faulty_power += &new_partition_faulty_power;
            if !new_faults.is_empty() {
                partitions_with_fault.push(partition_idx);
            }

            partitions
                .set(partition_idx, partition)
                .map_err(|e| actor_error!(ErrIllegalState; "failed to store partition {}: {:?}", partition_idx, e))?;
        }

        self.partitions = partitions.flush().map_err(
            |e| actor_error!(ErrIllegalState; "failed to store partitions root: {:?}", e),
        )?;

        self.add_expiration_partitions(store, fault_expiration_epoch, &partitions_with_fault, quant)
            .map_err(|e| actor_error!(ErrIllegalState; "failed to update expirations for partitions with faults: {:?}", e))?;

        self.faulty_power += &new_faulty_power;
        Ok(new_faulty_power)
    }

    pub fn declare_faults_recovered<BS: BlockStore>(
        &mut self,
        store: &BS,
        sectors: &Sectors<'_, BS>,
        sector_size: SectorSize,
        partition_sectors: &PartitionSectorMap,
    ) -> Result<(), ActorError> {
        let mut partitions = self.partitions_amt(store)?;

        for (partition_idx, sector_numbers) in partition_sectors.iter() {
            let mut partition = partitions
                .get(partition_idx)
                .map_err(
                    |e| actor_error!(ErrIllegalState; "failed to load partition {}: {:?}", partition_idx, e),
                )?
                .ok_or_else(|| actor_error!(ErrNotFound; "no such partition {}", partition_idx))?.clone();

            partition
                .declare_faults_recovered(sectors, sector_size, sector_numbers)
                .map_err(|e| actor_error!(ErrIllegalState; "failed to add recoveries: {:?}", e))?;

            partitions
                .set(partition_idx, partition)
                .map_err(|e| actor_error!(ErrIllegalState; "failed to update partition {}: {:?}", partition_idx, e))?;
        }

        // Power is not regained until the deadline end, when the recovery is confirmed.

        self.partitions = partitions.flush().map_err(
            |e| actor_error!(ErrIllegalState; "failed to store partitions root: {:?}", e),
        )?;

        Ok(())
    }

    /// Processes all PoSt submissions, marking unproven sectors as faulty and clearing failed recoveries.
    /// Returns any new faulty power and failed recovery power.
    pub fn process_deadline_end<BS: BlockStore>(
        &mut self,
        store: &BS,
        quant: QuantSpec,
        fault_expiration_epoch: ChainEpoch,
    ) -> Result<(PowerPair, PowerPair), ActorError> {
        let mut new_faulty_power = PowerPair::zero();
        let mut failed_recovery_power = PowerPair::zero();

        let mut partitions = self
            .partitions_amt(store)
            .map_err(|e| actor_error!(ErrIllegalState; "failed to load partitions: {:?}", e))?;

        let mut detected_any = false;
        let mut rescheduled_partitions = Vec::<u64>::new();

        for partition_idx in 0..partitions.count() {
            let proven = self.post_submissions.get(partition_idx as usize);

            if proven {
                continue;
            }

            let mut partition = partitions
                .get(partition_idx)
                .map_err(|e| {
                    actor_error!(
                        ErrIllegalState,
                        "failed to load partition {}: {:?}",
                        partition_idx,
                        e
                    )
                })?
                .ok_or_else(|| actor_error!(ErrIllegalState; "no partition {}", partition_idx))?
                .clone();

            // If we have no recovering power/sectors, and all power is faulty, skip
            // this. This lets us skip some work if a miner repeatedly fails to PoSt.
            if partition.recovering_power.is_zero()
                && partition.faulty_power == partition.live_power
            {
                continue;
            }

            // Ok, we actually need to process this partition. Make sure we save the partition state back.
            detected_any = true;

            let (part_faulty_power, part_failed_recovery_power) = partition
                .record_missed_post(store, fault_expiration_epoch, quant)
                .map_err(|e| actor_error!(ErrIllegalState; "failed to record missed PoSt for partition {}: {:?}", partition_idx, e))?;

            // We marked some sectors faulty, we need to record the new
            // expiration. We don't want to do this if we're just penalizing
            // the miner for failing to recover power.
            if !part_faulty_power.is_zero() {
                rescheduled_partitions.push(partition_idx);
            }

            // Save new partition state.
            partitions
                .set(partition_idx, partition)
                .map_err(|e| actor_error!(ErrIllegalState; "failed to update partition {}: {:?}", partition_idx, e))?;

            new_faulty_power += &part_faulty_power;
            failed_recovery_power += &part_failed_recovery_power;
        }

        // Save modified deadline state.
        if detected_any {
            self.partitions = partitions.flush().map_err(
                |e| actor_error!(ErrIllegalState; "failed to store partitions: {:?}", e),
            )?;
        }

        self.add_expiration_partitions(
            store,
            fault_expiration_epoch,
            &rescheduled_partitions,
            quant,
        )
        .map_err(|e| actor_error!(ErrIllegalState; "failed to update deadline expiration queue: {:?}", e))?;

        self.faulty_power += &new_faulty_power;

        // Reset PoSt submissions.
        self.post_submissions = BitField::new();
        Ok((new_faulty_power, failed_recovery_power))
    }
}

pub struct PoStResult {
    pub new_faulty_power: PowerPair,
    pub retracted_recovery_power: PowerPair,
    pub recovered_power: PowerPair,
    /// A bitfield of all sectors in the proven partitions.
    pub sectors: BitField,
    /// A subset of `sectors` that should be ignored.
    pub ignored_sectors: BitField,
}

impl PoStResult {
    /// The power change (positive or negative) after processing the PoSt submission.
    pub fn power_delta(&self) -> PowerPair {
        &self.recovered_power - &self.new_faulty_power
    }

    /// The power from this PoSt that should be penalized.
    pub fn penalty_power(&self) -> PowerPair {
        &self.new_faulty_power + &self.retracted_recovery_power
    }
}

impl Deadline {
    /// Processes a series of posts, recording proven partitions and marking skipped
    /// sectors as faulty.
    ///
    /// It returns a PoStResult containing the list of proven and skipped sectors and
    /// changes to power (newly faulty power, power that should have been proven
    /// recovered but wasn't, and newly recovered power).
    ///
    /// NOTE: This function does not actually _verify_ any proofs. The returned
    /// `sectors` and `ignored_sectors` must subsequently be validated against the PoSt
    /// submitted by the miner.
    pub fn record_proven_sectors<BS: BlockStore>(
        &mut self,
        store: &BS,
        sectors: &Sectors<'_, BS>,
        sector_size: SectorSize,
        quant: QuantSpec,
        fault_expiration: ChainEpoch,
        post_partitions: &[PoStPartition],
    ) -> Result<PoStResult, Box<dyn StdError>> {
        let mut partitions = self.partitions_amt(store)?;

        let mut all_sectors = Vec::<BitField>::with_capacity(post_partitions.len());
        let mut all_ignored = Vec::<BitField>::with_capacity(post_partitions.len());
        let mut new_faulty_power_total = PowerPair::zero();
        let mut retracted_recovery_power_total = PowerPair::zero();
        let mut recovered_power_total = PowerPair::zero();
        let mut rescheduled_partitions = Vec::<u64>::new();

        // Accumulate sectors info for proof verification.
        for post in post_partitions {
            let already_proven = self.post_submissions.get(post.index as usize);

            if already_proven {
                // Skip partitions already proven for this deadline.
                continue;
            }

            let mut partition = partitions
                .get(post.index)
                .map_err(|e| format!("failed to load partition {}: {}", post.index, e))?
                .ok_or_else(|| actor_error!(ErrNotFound; "no such partition {}", post.index))?
                .clone();

            // Process new faults and accumulate new faulty power.
            // This updates the faults in partition state ahead of calculating the sectors to include for proof.
            let (new_fault_power, retracted_recovery_power) = partition
                .record_skipped_faults(
                    store,
                    sectors,
                    sector_size,
                    quant,
                    fault_expiration,
                    &post.skipped,
                )
                .map_err(|e| {
                    e.wrap(format!(
                        "failed to add skipped faults to partition {}",
                        post.index
                    ))
                })?;

            // If we have new faulty power, we've added some faults. We need
            // to record the new expiration in the deadline.
            if !new_fault_power.is_zero() {
                rescheduled_partitions.push(post.index);
            }

            let recovered_power = partition
                .recover_faults(store, sectors, sector_size, quant)
                .map_err(|e| {
                    ActorError::downcast_wrap(
                        e,
                        format!(
                            "failed to recover faulty sectors for partition {}",
                            post.index
                        ),
                    )
                })?;

            // note: we do this first because `partition` is moved in the upcoming `partitions.set` call
            // At this point, the partition faults represents the expected faults for the proof, with new skipped
            // faults and recoveries taken into account.
            all_sectors.push(partition.sectors.clone());
            all_ignored.push(partition.faults.clone());
            all_ignored.push(partition.terminated.clone());

            // This will be rolled back if the method aborts with a failed proof.
            partitions
                .set(post.index, partition)
                .map_err(|e| actor_error!(ErrIllegalState; "failed to update partition {}: {:?}", post.index, e))?;

            new_faulty_power_total += &new_fault_power;
            retracted_recovery_power_total += &retracted_recovery_power;
            recovered_power_total += &recovered_power;

            // Record the post.
            self.post_submissions.set(post.index as usize);
        }

        self.add_expiration_partitions(store, fault_expiration, &rescheduled_partitions, quant)
            .map_err(|e| actor_error!(ErrIllegalState; "failed to update expirations for partitions with faults: {:?}", e))?;

        // Save everything back.
        self.faulty_power -= &recovered_power_total;
        self.faulty_power += &new_faulty_power_total;

        self.partitions = partitions
            .flush()
            .map_err(|e| actor_error!(ErrIllegalState; "failed to persist partitions: {:?}", e))?;

        // Collect all sectors, faults, and recoveries for proof verification.
        let all_sector_numbers = BitField::union(&all_sectors);
        let all_ignored_sector_numbers = BitField::union(&all_ignored);

        Ok(PoStResult {
            new_faulty_power: new_faulty_power_total,
            retracted_recovery_power: retracted_recovery_power_total,
            recovered_power: recovered_power_total,
            sectors: all_sector_numbers,
            ignored_sectors: all_ignored_sector_numbers,
        })
    }

    /// RescheduleSectorExpirations reschedules the expirations of the given sectors
    /// to the target epoch, skipping any sectors it can't find.
    ///
    /// The power of the rescheduled sectors is assumed to have not changed since
    /// initial scheduling.
    ///
    /// Note: see the docs on State.RescheduleSectorExpirations for details on why we
    /// skip sectors/partitions we can't find.
    pub fn reschedule_sector_expirations<BS: BlockStore>(
        &mut self,
        store: &BS,
        sectors: &Sectors<'_, BS>,
        expiration: ChainEpoch,
        partition_sectors: &PartitionSectorMap,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> Result<(), Box<dyn StdError>> {
        let mut partitions = self.partitions_amt(store)?;

        // track partitions with moved expirations.
        let mut rescheduled_partitions = Vec::<u64>::new();

        for (partition_idx, sector_numbers) in partition_sectors.iter() {
            let mut partition = match partitions
                .get(partition_idx)
                .map_err(|e| format!("failed to load partition {}: {:?}", partition_idx, e))?
            {
                Some(partition) => partition.clone(),
                None => {
                    // We failed to find the partition, it could have moved
                    // due to compaction. This function is only reschedules
                    // sectors it can find so we'll just skip it.
                    continue;
                }
            };

            let moved = partition
                .reschedule_expirations(
                    store,
                    sectors,
                    expiration,
                    sector_numbers,
                    sector_size,
                    quant,
                )
                .map_err(|e| {
                    ActorError::downcast_wrap(
                        e,
                        format!(
                            "failed to reschedule expirations in partition {}",
                            partition_idx
                        ),
                    )
                })?;

            if moved.is_empty() {
                // nothing moved.
                continue;
            }

            rescheduled_partitions.push(partition_idx);
            partitions
                .set(partition_idx, partition)
                .map_err(|e| format!("failed to store partition {}: {:?}", partition_idx, e))?;
        }

        if !rescheduled_partitions.is_empty() {
            self.partitions = partitions
                .flush()
                .map_err(|e| format!("failed to save partitions: {:?}", e))?;

            self.add_expiration_partitions(store, expiration, &rescheduled_partitions, quant)
                .map_err(|e| format!("failed to reschedule partition expirations: {}", e))?;
        }

        Ok(())
    }
}
