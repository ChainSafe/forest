// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{
    BitFieldQueue, ExpirationSet, Partition, PartitionSectorMap, PoStPartition, PowerPair,
    SectorOnChainInfo, Sectors, TerminationResult, WPOST_PERIOD_DEADLINES,
};
use crate::{actor_error, ActorDowncast, ActorError, ExitCode, TokenAmount};
use bitfield::BitField;
use cid::{Cid, Code::Blake2b256};
use clock::ChainEpoch;
use encoding::tuple::*;
use fil_types::{deadlines::QuantSpec, PoStProof, SectorSize};
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use num_traits::{Signed, Zero};
use std::{cmp, collections::HashMap, collections::HashSet, error::Error as StdError};

// Bitwidth of AMTs determined empirically from mutation patterns and projections of mainnet data.
const DEADLINE_PARTITIONS_AMT_BITWIDTH: usize = 3; // Usually a small array
const DEADLINE_EXPIRATIONS_AMT_BITWIDTH: usize = 5;

// Given that 4 partitions can be proven in one post, this AMT's height will
// only exceed the partition AMT's height at ~0.75EiB of storage.
const DEADLINE_OPTIMISTIC_POST_SUBMISSIONS_AMT_BITWIDTH: usize = 2;

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
        deadline_idx: usize,
    ) -> Result<Deadline, ActorError> {
        if deadline_idx >= WPOST_PERIOD_DEADLINES as usize {
            return Err(actor_error!(
                ErrIllegalArgument,
                "invalid deadline {}",
                deadline_idx
            ));
        }

        store
            .get(&self.due[deadline_idx])
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
        mut f: impl FnMut(usize, Deadline) -> Result<(), Box<dyn StdError>>,
    ) -> Result<(), Box<dyn StdError>> {
        for i in 0..self.due.len() {
            let index = i;
            let deadline = self.load_deadline(store, index)?;
            f(index, deadline)?;
        }
        Ok(())
    }

    pub fn update_deadline<BS: BlockStore>(
        &mut self,
        store: &BS,
        deadline_idx: usize,
        deadline: &Deadline,
    ) -> Result<(), Box<dyn StdError>> {
        if deadline_idx >= WPOST_PERIOD_DEADLINES as usize {
            return Err(format!("invalid deadline {}", deadline_idx).into());
        }

        deadline.validate_state()?;

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

    // Partitions that have been proved by window PoSts so far during the
    // current challenge window.
    // NOTE: This bitfield includes both partitions whose proofs
    // were optimistically accepted and stored in
    // OptimisticPoStSubmissions, and those whose proofs were
    // verified on-chain.
    pub partitions_posted: BitField,

    /// Partitions with sectors that terminated early.
    pub early_terminations: BitField,

    /// The number of non-terminated sectors in this deadline (incl faulty).
    pub live_sectors: u64,

    /// The total number of sectors in this deadline (incl dead).
    pub total_sectors: u64,

    /// Memoized sum of faulty power in partitions.
    pub faulty_power: PowerPair,

    // AMT of optimistically accepted WindowPoSt proofs, submitted during
    // the current challenge window. At the end of the challenge window,
    // this AMT will be moved to PoStSubmissionsSnapshot. WindowPoSt proofs
    // verified on-chain do not appear in this AMT
    pub optimistic_post_submissions: Cid,

    // Snapshot of partition state at the end of the previous challenge
    // window for this deadline.
    partitions_snapshot: Cid,

    // Snapshot of the proofs submitted by the end of the previous challenge
    // window for this deadline.
    //
    // These proofs may be disputed via DisputeWindowedPoSt. Successfully
    // disputed window PoSts are removed from the snapshot.
    optimistic_post_submissions_snapshot: Cid,
}
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct WindowedPoSt {
    // Partitions proved by this WindowedPoSt.
    partitions: BitField,

    // Array of proofs, one per distinct registered proof type present in
    // the sectors being proven. In the usual case of a single proof type,
    // this array will always have a single element (independent of number
    // of partitions).
    proofs: Vec<PoStProof>,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct DisputeInfo {
    pub all_sector_nos: BitField,
    pub ignored_sector_nos: BitField,
    pub disputed_sectors: PartitionSectorMap,
    pub disputed_power: PowerPair,
}

impl Deadline {
    pub fn new<BS: BlockStore>(store: &BS) -> Result<Self, Box<dyn StdError>> {
        let empty_partitions_array =
            Amt::<(), BS>::new_with_bit_width(store, DEADLINE_PARTITIONS_AMT_BITWIDTH)
                .flush()
                .map_err(|e| format!("Failed to create empty states array: {}", e))?;
        let empty_deadline_expiration_array =
            Amt::<(), BS>::new_with_bit_width(store, DEADLINE_EXPIRATIONS_AMT_BITWIDTH)
                .flush()
                .map_err(|e| format!("Failed to create empty states array: {}", e))?;
        let empty_post_submissions_array = Amt::<(), BS>::new_with_bit_width(
            store,
            DEADLINE_OPTIMISTIC_POST_SUBMISSIONS_AMT_BITWIDTH,
        )
        .flush()
        .map_err(|e| format!("Failed to create empty states array: {}", e))?;
        Ok(Self {
            partitions: empty_partitions_array,
            expirations_epochs: empty_deadline_expiration_array,
            early_terminations: BitField::new(),
            live_sectors: 0,
            total_sectors: 0,
            faulty_power: PowerPair::zero(),
            partitions_posted: BitField::new(),
            optimistic_post_submissions: empty_post_submissions_array,
            partitions_snapshot: empty_partitions_array,
            optimistic_post_submissions_snapshot: empty_post_submissions_array,
        })
    }

    pub fn partitions_amt<'db, BS: BlockStore>(
        &self,
        store: &'db BS,
    ) -> Result<Amt<'db, Partition, BS>, Box<dyn StdError>> {
        Ok(Amt::load(&self.partitions, store)?)
    }

    pub fn optimistic_proofs_amt<'db, BS: BlockStore>(
        &self,
        store: &'db BS,
    ) -> Result<Amt<'db, WindowedPoSt, BS>, Box<dyn StdError>> {
        Ok(Amt::load(&self.optimistic_post_submissions, store)?)
    }

    pub fn partitions_snapshot_amt<'db, BS: BlockStore>(
        &self,
        store: &'db BS,
    ) -> Result<Amt<'db, Partition, BS>, Box<dyn StdError>> {
        Ok(Amt::load(&self.partitions_snapshot, store)?)
    }

    pub fn optimistic_proofs_snapshot_amt<'db, BS: BlockStore>(
        &self,
        store: &'db BS,
    ) -> Result<Amt<'db, WindowedPoSt, BS>, Box<dyn StdError>> {
        Ok(Amt::load(
            &self.optimistic_post_submissions_snapshot,
            store,
        )?)
    }

    pub fn load_partition<BS: BlockStore>(
        &self,
        store: &BS,
        partition_idx: usize,
    ) -> Result<Partition, Box<dyn StdError>> {
        let partitions = Amt::<Partition, _>::load(&self.partitions, store)?;

        let partition = partitions
            .get(partition_idx)
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    format!("failed to lookup partition {}", partition_idx),
                )
            })?
            .ok_or_else(|| actor_error!(ErrNotFound, "no partition {}", partition_idx))?;

        Ok(partition.clone())
    }

    pub fn load_partition_snapshot<BS: BlockStore>(
        &self,
        store: &BS,
        partition_idx: usize,
    ) -> Result<Partition, Box<dyn StdError>> {
        let partitions = Amt::<Partition, _>::load(&self.partitions_snapshot, store)?;

        let partition = partitions
            .get(partition_idx)
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    format!("failed to lookup partition snapshot {}", partition_idx),
                )
            })?
            .ok_or_else(|| actor_error!(ErrNotFound, "no partition snapshot {}", partition_idx))?;

        Ok(partition.clone())
    }

    /// Adds some partition numbers to the set expiring at an epoch.
    pub fn add_expiration_partitions<BS: BlockStore>(
        &mut self,
        store: &BS,
        expiration_epoch: ChainEpoch,
        partitions: &[usize],
        quant: QuantSpec,
    ) -> Result<(), Box<dyn StdError>> {
        // Avoid doing any work if there's nothing to reschedule.
        if partitions.is_empty() {
            return Ok(());
        }

        let mut queue = BitFieldQueue::new(store, &self.expirations_epochs, quant)
            .map_err(|e| e.downcast_wrap("failed to load expiration queue"))?;
        queue
            .add_to_queue_values(expiration_epoch, partitions)
            .map_err(|e| e.downcast_wrap("failed to mutate expiration queue"))?;
        self.expirations_epochs = queue
            .amt
            .flush()
            .map_err(|e| e.downcast_wrap("failed to save expiration queue"))?;

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
        let mut partitions_with_early_terminations = Vec::<usize>::new();

        // For each partition with an expiry, remove and collect expirations from the partition queue.
        for i in expired_partitions.iter() {
            let partition_idx = i;
            let mut partition = partitions
                .get(partition_idx)?
                .cloned()
                .ok_or_else(|| format!("missing expected partition {}", partition_idx))?;

            let partition_expiration =
                partition
                    .pop_expired_sectors(store, until, quant)
                    .map_err(|e| {
                        e.downcast_wrap(format!(
                            "failed to pop expired sectors from partition {}",
                            partition_idx
                        ))
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
        proven: bool,
        mut sectors: &[SectorOnChainInfo],
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> Result<PowerPair, Box<dyn StdError>> {
        let mut total_power = PowerPair::zero();
        if sectors.is_empty() {
            return Ok(total_power);
        }

        // First update partitions, consuming the sectors
        let mut partition_deadline_updates = HashMap::<ChainEpoch, Vec<usize>>::new();
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
                    Partition::new(store)?
                }
            };

            // Figure out which (if any) sectors we want to add to this partition.
            let sector_count = partition.sectors.len() as u64;
            if sector_count >= partition_size {
                continue;
            }

            let size = cmp::min(partition_size - sector_count, sectors.len() as u64) as usize;
            let partition_new_sectors = &sectors[..size];

            // Intentionally ignoring the index at size, split_at returns size inclusively for start
            sectors = &sectors[size..];

            // Add sectors to partition.
            let partition_power =
                partition.add_sectors(store, proven, partition_new_sectors, sector_size, quant)?;
            total_power += &partition_power;

            // Save partition back.
            partitions.set(partition_idx, partition)?;

            // Record deadline -> partition mapping so we can later update the deadlines.
            for sector in partition_new_sectors {
                let partition_update = partition_deadline_updates
                    .entry(sector.expiration)
                    .or_default();
                if partition_update.is_empty() || partition_update.last() != Some(&partition_idx) {
                    partition_update.push(partition_idx);
                }
            }
        }

        // Save partitions back.
        self.partitions = partitions.flush()?;

        // Next, update the expiration queue.
        let mut deadline_expirations =
            BitFieldQueue::new(store, &self.expirations_epochs, quant)
                .map_err(|e| e.downcast_wrap("failed to load expiration epochs"))?;
        deadline_expirations
            .add_many_to_queue_values(&partition_deadline_updates)
            .map_err(|e| e.downcast_wrap("failed to add expirations for new deadlines"))?;
        self.expirations_epochs = deadline_expirations.amt.flush()?;

        Ok(total_power)
    }

    pub fn pop_early_terminations<BS: BlockStore>(
        &mut self,
        store: &BS,
        max_partitions: u64,
        max_sectors: u64,
    ) -> Result<(TerminationResult, /* has more */ bool), Box<dyn StdError>> {
        let mut partitions = self.partitions_amt(store)?;

        let mut partitions_finished = Vec::<usize>::new();
        let mut result = TerminationResult::new();

        for i in self.early_terminations.iter() {
            let partition_idx = i;

            let mut partition = match partitions.get(partition_idx).map_err(|e| {
                e.downcast_wrap(format!("failed to load partition {}", partition_idx))
            })? {
                Some(partition) => partition.clone(),
                None => {
                    partitions_finished.push(partition_idx);
                    continue;
                }
            };

            // Pop early terminations.
            let (partition_result, more) = partition
                .pop_early_terminations(store, max_sectors - result.sectors_processed)
                .map_err(|e| e.downcast_wrap("failed to pop terminations from partition"))?;

            result += partition_result;

            // If we've processed all of them for this partition, unmark it in the deadline.
            if !more {
                partitions_finished.push(partition_idx);
            }

            // Save partition
            partitions.set(partition_idx, partition).map_err(|e| {
                e.downcast_wrap(format!("failed to store partition {}", partition_idx))
            })?;

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
            .map_err(|e| e.downcast_wrap("failed to update partitions"))?;

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
            .map_err(|e| e.downcast_wrap("failed to pop expiring partitions"))?;

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
        partition_sectors: &mut PartitionSectorMap,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> Result<PowerPair, Box<dyn StdError>> {
        let mut partitions = self.partitions_amt(store)?;

        let mut power_lost = PowerPair::zero();
        for (partition_idx, sector_numbers) in partition_sectors.iter() {
            let mut partition = partitions
                .get(partition_idx)
                .map_err(|e| {
                    e.downcast_wrap(format!("failed to load partition {}", partition_idx))
                })?
                .ok_or_else(
                    || actor_error!(ErrNotFound; "failed to find partition {}", partition_idx),
                )?
                .clone();

            let removed = partition
                .terminate_sectors(store, sectors, epoch, sector_numbers, sector_size, quant)
                .map_err(|e| {
                    e.downcast_wrap(format!(
                        "failed to terminate sectors in partition {}",
                        partition_idx
                    ))
                })?;

            partitions.set(partition_idx, partition).map_err(|e| {
                e.downcast_wrap(format!(
                    "failed to store updated partition {}",
                    partition_idx
                ))
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
            .map_err(|e| e.downcast_wrap("failed to persist partitions"))?;

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
            .map_err(|e| e.downcast_wrap("failed to load partitions"))?;

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

        if let Some(partition_idx) = to_remove_set.iter().find(|&&i| i >= partition_count) {
            return Err(
                actor_error!(ErrIllegalArgument; "partition index {} out of range [0, {})", partition_idx, partition_count).into()
            );
        }

        // Should already be checked earlier, but we might as well check again.
        if !self.early_terminations.is_empty() {
            return Err("cannot remove partitions from deadline with early terminations".into());
        }

        let mut new_partitions =
            Amt::<Partition, BS>::new_with_bit_width(store, DEADLINE_PARTITIONS_AMT_BITWIDTH);
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
                    return Err(actor_error!(
                        ErrIllegalArgument,
                        "cannot remove partition {}: has faults",
                        partition_idx
                    )
                    .into());
                }

                // Don't allow removing partitions with unproven sectors
                let all_proven = partition.unproven.is_empty();
                if !all_proven {
                    return Err(actor_error!(
                        ErrIllegalArgument,
                        "cannot remove partition {}: has unproven sectors",
                        partition_idx
                    )
                    .into());
                }

                // Get the live sectors.
                let live_sectors = partition.live_sectors();

                all_dead_sectors.push(partition.terminated.clone());
                all_live_sectors.push(live_sectors);
                removed_power += &partition.live_power;

                Ok(())
            })
            .map_err(|e| e.downcast_wrap("while removing partitions"))?;

        self.partitions = new_partitions
            .flush()
            .map_err(|e| e.downcast_wrap("failed to persist new partition table"))?;

        let dead = BitField::union(&all_dead_sectors);
        let live = BitField::union(&all_live_sectors);

        // Update sector counts.
        let removed_dead_sectors = dead.len() as u64;
        let removed_live_sectors = live.len() as u64;

        self.live_sectors -= removed_live_sectors;
        self.total_sectors -= removed_live_sectors + removed_dead_sectors;

        // Update expiration bitfields.
        let mut expiration_epochs = BitFieldQueue::new(store, &self.expirations_epochs, quant)
            .map_err(|e| e.downcast_wrap("failed to load expiration queue"))?;

        expiration_epochs.cut(to_remove).map_err(|e| {
            e.downcast_wrap("failed cut removed partitions from deadline expiration queue")
        })?;

        self.expirations_epochs = expiration_epochs
            .amt
            .flush()
            .map_err(|e| e.downcast_wrap("failed persist deadline expiration queue"))?;

        Ok((live, dead, removed_power))
    }

    pub fn record_faults<BS: BlockStore>(
        &mut self,
        store: &BS,
        sectors: &Sectors<'_, BS>,
        sector_size: SectorSize,
        quant: QuantSpec,
        fault_expiration_epoch: ChainEpoch,
        partition_sectors: &mut PartitionSectorMap,
    ) -> Result<PowerPair, Box<dyn StdError>> {
        let mut partitions = self.partitions_amt(store)?;

        // Record partitions with some fault, for subsequently indexing in the deadline.
        // Duplicate entries don't matter, they'll be stored in a bitfield (a set).
        let mut partitions_with_fault = Vec::<usize>::with_capacity(partition_sectors.len());
        let mut power_delta = PowerPair::zero();

        for (partition_idx, sector_numbers) in partition_sectors.iter() {
            let mut partition = partitions
                .get(partition_idx)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to load partition {}", partition_idx),
                    )
                })?
                .ok_or_else(|| actor_error!(ErrNotFound; "no such partition {}", partition_idx))?
                .clone();

            let (new_faults, partition_power_delta, partition_new_faulty_power) = partition
                .record_faults(
                    store,
                    sectors,
                    sector_numbers,
                    fault_expiration_epoch,
                    sector_size,
                    quant,
                )
                .map_err(|e| {
                    e.downcast_wrap(format!(
                        "failed to declare faults in partition {}",
                        partition_idx
                    ))
                })?;

            self.faulty_power += &partition_new_faulty_power;
            power_delta += &partition_power_delta;
            if !new_faults.is_empty() {
                partitions_with_fault.push(partition_idx);
            }

            partitions.set(partition_idx, partition).map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    format!("failed to store partition {}", partition_idx),
                )
            })?;
        }

        self.partitions = partitions.flush().map_err(|e| {
            e.downcast_default(ExitCode::ErrIllegalState, "failed to store partitions root")
        })?;

        self.add_expiration_partitions(
            store,
            fault_expiration_epoch,
            &partitions_with_fault,
            quant,
        )
        .map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                "failed to update expirations for partitions with faults",
            )
        })?;

        Ok(power_delta)
    }

    pub fn declare_faults_recovered<BS: BlockStore>(
        &mut self,
        store: &BS,
        sectors: &Sectors<'_, BS>,
        sector_size: SectorSize,
        partition_sectors: &mut PartitionSectorMap,
    ) -> Result<(), Box<dyn StdError>> {
        let mut partitions = self.partitions_amt(store)?;

        for (partition_idx, sector_numbers) in partition_sectors.iter() {
            let mut partition = partitions
                .get(partition_idx)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to load partition {}", partition_idx),
                    )
                })?
                .ok_or_else(|| actor_error!(ErrNotFound; "no such partition {}", partition_idx))?
                .clone();

            partition
                .declare_faults_recovered(sectors, sector_size, sector_numbers)
                .map_err(|e| e.downcast_wrap("failed to add recoveries"))?;

            partitions.set(partition_idx, partition).map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    format!("failed to update partition {}", partition_idx),
                )
            })?;
        }

        // Power is not regained until the deadline end, when the recovery is confirmed.

        self.partitions = partitions.flush().map_err(|e| {
            e.downcast_default(ExitCode::ErrIllegalState, "failed to store partitions root")
        })?;

        Ok(())
    }

    /// Processes all PoSt submissions, marking unproven sectors as
    /// faulty and clearing failed recoveries. It returns the power delta, and any
    /// power that should be penalized (new faults and failed recoveries).
    pub fn process_deadline_end<BS: BlockStore>(
        &mut self,
        store: &BS,
        quant: QuantSpec,
        fault_expiration_epoch: ChainEpoch,
    ) -> Result<(PowerPair, PowerPair), ActorError> {
        let mut partitions = self.partitions_amt(store).map_err(|e| {
            e.downcast_default(ExitCode::ErrIllegalState, "failed to load partitions")
        })?;

        let mut detected_any = false;
        let mut rescheduled_partitions = Vec::<usize>::new();
        let mut power_delta = PowerPair::zero();
        let mut penalized_power = PowerPair::zero();
        for partition_idx in 0..partitions.count() {
            let proven = self.partitions_posted.get(partition_idx);

            if proven {
                continue;
            }

            let mut partition = partitions
                .get(partition_idx)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to load partition {}", partition_idx),
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

            let (part_power_delta, part_penalized_power, part_new_faulty_power) = partition
                .record_missed_post(store, fault_expiration_epoch, quant)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!(
                            "failed to record missed PoSt for partition {}",
                            partition_idx
                        ),
                    )
                })?;

            // We marked some sectors faulty, we need to record the new
            // expiration. We don't want to do this if we're just penalizing
            // the miner for failing to recover power.
            if !part_new_faulty_power.is_zero() {
                rescheduled_partitions.push(partition_idx);
            }

            // Save new partition state.
            partitions.set(partition_idx, partition).map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    format!("failed to update partition {}", partition_idx),
                )
            })?;

            self.faulty_power += &part_new_faulty_power;

            power_delta += &part_power_delta;
            penalized_power += &part_penalized_power;
        }

        // Save modified deadline state.
        if detected_any {
            self.partitions = partitions.flush().map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to store partitions")
            })?;
        }

        self.add_expiration_partitions(
            store,
            fault_expiration_epoch,
            &rescheduled_partitions,
            quant,
        )
        .map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                "failed to update deadline expiration queue",
            )
        })?;

        // Reset PoSt submissions.
        self.partitions_posted = BitField::new();
        self.partitions_snapshot = self.partitions;
        self.optimistic_post_submissions_snapshot = self.optimistic_post_submissions;
        self.optimistic_post_submissions = Amt::<(), BS>::new_with_bit_width(
            store,
            DEADLINE_OPTIMISTIC_POST_SUBMISSIONS_AMT_BITWIDTH,
        )
        .flush()
        .map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                "failed to clear pending proofs array",
            )
        })?;
        Ok((power_delta, penalized_power))
    }
    pub fn for_each<BS: BlockStore>(
        &self,
        store: &BS,
        f: impl FnMut(usize, &Partition) -> Result<(), Box<dyn StdError>>,
    ) -> Result<(), Box<dyn StdError>> {
        let parts = self.partitions_amt(store)?;
        parts.for_each(f)
    }

    pub fn validate_state(&self) -> Result<(), &'static str> {
        if self.live_sectors > self.total_sectors {
            return Err("deadline left with more live sectors than total");
        }

        if self.faulty_power.raw.is_negative() || self.faulty_power.qa.is_negative() {
            return Err("deadline left with negative faulty power");
        }

        Ok(())
    }

    pub fn load_partitions_for_dispute<BS: BlockStore>(
        &self,
        store: &BS,
        partitions: BitField,
    ) -> Result<DisputeInfo, Box<dyn StdError>> {
        let partitions_snapshot = self
            .partitions_snapshot_amt(store)
            .map_err(|e| e.downcast_wrap("failed to load partitions {}"))?;

        let mut all_sectors = Vec::new();
        let mut all_ignored = Vec::new();
        let mut disputed_sectors = PartitionSectorMap::default();
        let mut disputed_power = PowerPair::zero();
        for part_idx in partitions.iter() {
            let partition_snapshot = partitions_snapshot
                .get(part_idx)?
                .ok_or_else(|| format!("failed to find partition {}", part_idx))?;

            // Record sectors for proof verification
            all_sectors.push(partition_snapshot.sectors.clone());
            all_ignored.push(partition_snapshot.faults.clone());
            all_ignored.push(partition_snapshot.terminated.clone());
            all_ignored.push(partition_snapshot.unproven.clone());

            // Record active sectors for marking faults.
            let active = partition_snapshot.active_sectors();
            disputed_sectors.add(part_idx, active.into())?;

            // Record disputed power for penalties.
            //
            // NOTE: This also includes power that was
            // activated at the end of the last challenge
            // window, and power from sectors that have since
            // expired.
            disputed_power += &partition_snapshot.active_power();
        }

        let all_sector_nos = BitField::union(&all_sectors);
        let all_ignored_nos = BitField::union(&all_ignored);

        Ok(DisputeInfo {
            all_sector_nos,
            disputed_sectors,
            disputed_power,
            ignored_sector_nos: all_ignored_nos,
        })
    }

    pub fn is_live(&self) -> bool {
        if self.live_sectors > 0 {
            return true;
        }

        let has_no_proofs = self.partitions_posted.is_empty();
        if !has_no_proofs {
            // _This_ case should be impossible, but there's no good way to log from here. We
            // might as well just process the deadline end and move on.
            return true;
        }

        // If the partitions have changed, we may have work to do. We should at least update the
        // partitions snapshot one last time.
        if self.partitions != self.partitions_snapshot {
            return true;
        }

        // If we don't have any proofs, and the proofs snapshot isn't the same as the current proofs
        // snapshot (which should be empty), we should update the deadline one last time to empty
        // the proofs snapshot.
        if self.optimistic_post_submissions != self.optimistic_post_submissions_snapshot {
            return true;
        }
        false
    }
}

pub struct PoStResult {
    /// Power activated or deactivated (positive or negative).
    pub power_delta: PowerPair,
    pub new_faulty_power: PowerPair,
    pub retracted_recovery_power: PowerPair,
    pub recovered_power: PowerPair,
    /// A bitfield of all sectors in the proven partitions.
    pub sectors: BitField,
    /// A subset of `sectors` that should be ignored.
    pub ignored_sectors: BitField,
    // Bitfield of partitions that were proven.
    pub partitions: BitField,
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
        post_partitions: &mut [PoStPartition],
    ) -> Result<PoStResult, Box<dyn StdError>> {
        let mut partition_indexes = BitField::new();
        for p in post_partitions.iter() {
            partition_indexes.set(p.index);
        }

        let num_partitions = partition_indexes.len();
        if num_partitions != post_partitions.len() {
            return Err(Box::new(actor_error!(
                ErrIllegalArgument,
                "duplicate partitions proven"
            )));
        }

        // First check to see if we're proving any already proven partitions.
        // This is faster than checking one by one.
        let already_proven = &self.partitions_posted & &partition_indexes;
        if !already_proven.is_empty() {
            return Err(Box::new(actor_error!(
                ErrIllegalArgument,
                "parition already proven: {:?}",
                already_proven
            )));
        }

        let mut partitions = self.partitions_amt(store)?;

        let mut all_sectors = Vec::<BitField>::with_capacity(post_partitions.len());
        let mut all_ignored = Vec::<BitField>::with_capacity(post_partitions.len());
        let mut new_faulty_power_total = PowerPair::zero();
        let mut retracted_recovery_power_total = PowerPair::zero();
        let mut recovered_power_total = PowerPair::zero();
        let mut rescheduled_partitions = Vec::<usize>::new();
        let mut power_delta = PowerPair::zero();

        // Accumulate sectors info for proof verification.
        for post in post_partitions {
            let mut partition = partitions
                .get(post.index)
                .map_err(|e| e.downcast_wrap(format!("failed to load partition {}", post.index)))?
                .ok_or_else(|| actor_error!(ErrNotFound; "no such partition {}", post.index))?
                .clone();

            // Process new faults and accumulate new faulty power.
            // This updates the faults in partition state ahead of calculating the sectors to include for proof.
            let (mut new_power_delta, new_fault_power, retracted_recovery_power, has_new_faults) =
                partition
                    .record_skipped_faults(
                        store,
                        sectors,
                        sector_size,
                        quant,
                        fault_expiration,
                        &mut post.skipped,
                    )
                    .map_err(|e| {
                        e.downcast_wrap(format!(
                            "failed to add skipped faults to partition {}",
                            post.index
                        ))
                    })?;

            // If we have new faulty power, we've added some faults. We need
            // to record the new expiration in the deadline.
            if has_new_faults {
                rescheduled_partitions.push(post.index);
            }

            let recovered_power = partition
                .recover_faults(store, sectors, sector_size, quant)
                .map_err(|e| {
                    e.downcast_wrap(format!(
                        "failed to recover faulty sectors for partition {}",
                        post.index
                    ))
                })?;

            new_power_delta += &partition.activate_unproven();

            // note: we do this first because `partition` is moved in the upcoming `partitions.set` call
            // At this point, the partition faults represents the expected faults for the proof, with new skipped
            // faults and recoveries taken into account.
            all_sectors.push(partition.sectors.clone());
            all_ignored.push(partition.faults.clone());
            all_ignored.push(partition.terminated.clone());

            // This will be rolled back if the method aborts with a failed proof.
            partitions.set(post.index, partition).map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    format!("failed to update partition {}", post.index),
                )
            })?;

            new_faulty_power_total += &new_fault_power;
            retracted_recovery_power_total += &retracted_recovery_power;
            recovered_power_total += &recovered_power;
            power_delta += &new_power_delta;
            power_delta += &recovered_power;

            // Record the post.
            self.partitions_posted.set(post.index as usize);
        }

        self.add_expiration_partitions(store, fault_expiration, &rescheduled_partitions, quant)
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to update expirations for partitions with faults",
                )
            })?;

        // Save everything back.
        self.faulty_power -= &recovered_power_total;
        self.faulty_power += &new_faulty_power_total;

        self.partitions = partitions.flush().map_err(|e| {
            e.downcast_default(ExitCode::ErrIllegalState, "failed to persist partitions")
        })?;

        // Collect all sectors, faults, and recoveries for proof verification.
        let all_sector_numbers = BitField::union(&all_sectors);
        let all_ignored_sector_numbers = BitField::union(&all_ignored);

        Ok(PoStResult {
            new_faulty_power: new_faulty_power_total,
            retracted_recovery_power: retracted_recovery_power_total,
            recovered_power: recovered_power_total,
            sectors: all_sector_numbers,
            power_delta,
            ignored_sectors: all_ignored_sector_numbers,
            partitions: partition_indexes,
        })
    }

    // RecordPoStProofs records a set of optimistically accepted PoSt proofs
    // (usually one), associating them with the given partitions.
    pub fn record_post_proofs<BS: BlockStore>(
        &mut self,
        store: &BS,
        partitions: &BitField,
        proofs: &[PoStProof],
    ) -> Result<(), Box<dyn StdError>> {
        let mut proof_arr = self
            .optimistic_proofs_amt(store)
            .map_err(|e| e.downcast_wrap("failed to load post proofs"))?;
        proof_arr
            .set(
                proof_arr.count(),
                // TODO: Can we do this with out cloning?
                WindowedPoSt {
                    partitions: partitions.clone(),
                    proofs: proofs.to_vec(),
                },
            )
            .map_err(|e| e.downcast_wrap("failed to store proof"))?;
        let root = proof_arr
            .flush()
            .map_err(|e| e.downcast_wrap("failed to save proofs"))?;
        self.optimistic_post_submissions = root;
        Ok(())
    }

    // TakePoStProofs removes and returns a PoSt proof by index, along with the
    // associated partitions. This method takes the PoSt from the PoSt submissions
    // snapshot.
    pub fn take_post_proofs<BS: BlockStore>(
        &mut self,
        store: &BS,
        idx: u64,
    ) -> Result<(BitField, Vec<PoStProof>), Box<dyn StdError>> {
        let mut proof_arr = self
            .optimistic_proofs_snapshot_amt(store)
            .map_err(|e| e.downcast_wrap("failed to load post proofs snapshot amt"))?;
        // Extract and remove the proof from the proofs array, leaving a hole.
        // This will not affect concurrent attempts to refute other proofs.
        let post = proof_arr
            .delete(idx as usize)
            .map_err(|e| e.downcast_wrap(format!("failed to retrieve proof {}", idx)))?
            .ok_or_else(|| actor_error!(ErrIllegalArgument, "proof {} not found", idx))?;

        let root = proof_arr
            .flush()
            .map_err(|e| e.downcast_wrap("failed to save proofs"))?;
        self.optimistic_post_submissions_snapshot = root;
        Ok((post.partitions, post.proofs))
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
        partition_sectors: &mut PartitionSectorMap,
        sector_size: SectorSize,
        quant: QuantSpec,
    ) -> Result<Vec<SectorOnChainInfo>, Box<dyn StdError>> {
        let mut partitions = self.partitions_amt(store)?;

        // track partitions with moved expirations.
        let mut rescheduled_partitions = Vec::<usize>::new();

        let mut all_replaced = Vec::new();
        for (partition_idx, sector_numbers) in partition_sectors.iter() {
            let mut partition = match partitions.get(partition_idx).map_err(|e| {
                e.downcast_wrap(format!("failed to load partition {}", partition_idx))
            })? {
                Some(partition) => partition.clone(),
                None => {
                    // We failed to find the partition, it could have moved
                    // due to compaction. This function is only reschedules
                    // sectors it can find so we'll just skip it.
                    continue;
                }
            };

            let replaced = partition
                .reschedule_expirations(
                    store,
                    sectors,
                    expiration,
                    sector_numbers,
                    sector_size,
                    quant,
                )
                .map_err(|e| {
                    e.downcast_wrap(format!(
                        "failed to reschedule expirations in partition {}",
                        partition_idx
                    ))
                })?;

            if replaced.is_empty() {
                // nothing moved.
                continue;
            }
            all_replaced.extend(replaced);

            rescheduled_partitions.push(partition_idx);
            partitions.set(partition_idx, partition).map_err(|e| {
                e.downcast_wrap(format!("failed to store partition {}", partition_idx))
            })?;
        }

        if !rescheduled_partitions.is_empty() {
            self.partitions = partitions
                .flush()
                .map_err(|e| e.downcast_wrap("failed to save partitions"))?;

            self.add_expiration_partitions(store, expiration, &rescheduled_partitions, quant)
                .map_err(|e| e.downcast_wrap("failed to reschedule partition expirations"))?;
        }

        Ok(all_replaced)
    }
}
