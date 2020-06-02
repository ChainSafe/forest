// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(unused_variables)]
#![allow(dead_code)]

use super::deadlines::{compute_proving_period_deadline, DeadlineInfo};
use super::policy::*;
use super::types::*;
use crate::{power, u64_key, BytesKey, OptionalEpoch, HAMT_BIT_WIDTH};
use address::Address;
use bitfield::BitField;
use cid::{multihash::Blake2b256, Cid};
use clock::ChainEpoch;
use encoding::{serde_bytes, tuple::*, Cbor};
use fil_types::{RegisteredProof, SectorInfo, SectorNumber, SectorSize};
use ipld_amt::{Amt, Error as AmtError};
use ipld_blockstore::BlockStore;
use ipld_hamt::{Error as HamtError, Hamt};
use num_bigint::bigint_ser;
use num_bigint::biguint_ser;
use num_bigint::{BigInt, BigUint};
use num_traits::Zero;
use vm::{TokenAmount, ActorError};
use num_traits::ToPrimitive;    

// Balance of Miner Actor should be greater than or equal to
// the sum of PreCommitDeposits and LockedFunds.
// Excess balance as computed by st.GetAvailableBalance will be
// withdrawable or usable for pre-commit deposit or pledge lock-up.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct State {
    /// Contains static info about this miner
    // TODO revisit as will likely change to Cid in future
    pub info: MinerInfo,

    /// Total funds locked as PreCommitDeposits
    #[serde(with = "biguint_ser")]
    pre_commit_deposit: TokenAmount,
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
    pub proving_period_start: OptionalEpoch,

    /// Sector numbers prove-committed since period start, to be added to Deadlines at next proving period boundary.
    pub new_sectors: BitField,

    /// Sector numbers indexed by expiry epoch (which are on proving period boundaries).
    /// Invariant: Keys(Sectors) == union(SectorExpirations.Values())
    /// Array, AMT[ChainEpoch]Bitfield
    sector_expirations: Cid,

    /// The sector numbers due for PoSt at each deadline in the current proving period, frozen at period start.
    /// New sectors are added and expired ones removed at proving period boundary.
    /// Faults are not subtracted from this in state, but on the fly.
    deadlines: Cid,

    /// All currently known faulty sectors, mutated eagerly.
    /// These sectors are exempt from inclusion in PoSt.
    pub faults: BitField,

    /// Faulty sector numbers indexed by the start epoch of the proving period in which detected.
    /// Used to track fault durations for eventual sector termination.
    /// At most 14 entries, b/c sectors faulty longer expire.
    /// Invariant: Faults == union(FaultEpochs.Values())
    /// AMT[ChainEpoch]Bitfield
    fault_epoch: Cid,

    /// Faulty sectors that will recover when next included in a valid PoSt.
    /// Invariant: Recoveries ⊆ Faults.
    recoveries: BitField,

    /// Records successful PoSt submission in the current proving period by partition number.
    /// The presence of a partition number indicates on-time PoSt received.
    post_submissions: BitField,

    /// The index of the next deadline for which faults should been detected and processed (after it's closed).
	/// The proving period cron handler will always reset this to 0, for the subsequent period.
	/// Eager fault detection processing on fault/recovery declarations or PoSt may set a smaller number,
	/// indicating partial progress, from which subsequent processing should continue.
	/// In the range [0, WPoStProvingPeriodDeadlines).
    next_deadline_to_process_faults: u64,
}

impl Cbor for State {}

impl State {
    pub fn new(
        empty_arr: Cid,
        empty_map: Cid,
        empty_deadlines: Cid,
        owner: Address,
        worker: Address,
        peer_id: Vec<u8>,
        proof_type: RegisteredProof,
    ) -> Self {
        let seal_proof_type = proof_type.registered_seal_proof();
        let sector_size = seal_proof_type.sector_size();
        let partitions_sectors = seal_proof_type.window_post_partitions_sector();
        Self {
            info: MinerInfo {
                owner,
                worker,
                pending_worker_key: None,
                peer_id,
                seal_proof_type,
                sector_size,
                window_post_partition_sectors: partitions_sectors,
            },
            pre_commit_deposit: TokenAmount::default(),
            locked_funds: TokenAmount::default(),
            vesting_funds: empty_arr.clone(),
            pre_committed_sectors: empty_map,
            sectors: empty_arr.clone(),
            proving_period_start: OptionalEpoch(None),
            new_sectors: BitField::default(),
            sector_expirations: empty_arr.clone(),
            deadlines: empty_deadlines,
            faults: BitField::default(),
            fault_epoch: empty_arr.clone(),
            recoveries: BitField::default(),
            post_submissions: BitField::default(),
            next_deadline_to_process_faults: 0,
        }
    }
    pub fn get_worker(&self) -> &Address {
        &self.info.worker
    }
    pub fn get_sector_size(&self) -> &SectorSize {
        &self.info.sector_size
    }
    // TODO update
    pub fn deadline_info(&self, current_epoch: ChainEpoch) -> Option<DeadlineInfo> {
        match compute_proving_period_deadline(self.proving_period_start, current_epoch) {
            Some(deadline_info) => Some(deadline_info),
            None => None,
        }
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
        match sectors.get(sector_num)? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
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
        sector_num: SectorNumber,
    ) -> Result<(), AmtError> {
        let mut sectors = Amt::<SectorOnChainInfo, _>::load(&self.sectors, store)?;
        sectors.delete(sector_num)?;

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
        let ns = BitField::new();
        for sector in sector_nos {
            ns.set(*sector)
        }
        self.new_sectors.merge(&ns)?;

        let count = self.new_sectors.count()?;
        if count as u64 > NEW_SECTORS_PER_PERIOD_MAX {
            return Err(format!("too many new sectors {}, max {}", count, NEW_SECTORS_PER_PERIOD_MAX));
        }

        Ok(())
    }
    /// Removes some sector numbers from the new sectors bitfield, if present.
    fn remove_new_sectors(&self, sector_nos: BitField) -> Result<(), String> {
        self.new_sectors.subtract(&sector_nos)?;
        Ok(())
    }
    /// Gets the sector numbers expiring at some epoch.
    fn get_sector_expirations<BS: BlockStore>(
        &self,
        store: &BS,
        expiry: ChainEpoch,
    ) -> Result<BitField, String> {
        let sectors = Amt::<SectorOnChainInfo, _>::load(&self.sectors, store)?;
        
    }
    // TODO
    /// Iterates sector expiration groups in order.
    /// Note that the sectors bitfield provided to the callback is not safe to store.
    fn for_each_sector_expiration<BS: BlockStore>(&self, store: &BS) {
        todo!()
    }
    /// Adds some sector numbers to the set expiring at an epoch.
    /// The sector numbers are given as uint64s to avoid pointless conversions.
    pub fn add_sector_expirations<BS: BlockStore>(
        &self,
        store: &BS,
        expiry: &ChainEpoch,
        sectors: &[u64],
    ) -> Result<(), String> {
        Ok(())
    }
    // TODO
    /// Removes some sector numbers from the set expiring at an epoch.
    fn remove_sector_expirations<BS: BlockStore>(
        &self,
        store: &BS,
        expiry: &ChainEpoch,
        sectors: &[u64],
    ) -> Result<(), String> {
        Ok(())
    }
    // TODO
    /// Removes all sector numbers from the set expiring some epochs.
    fn clear_sector_expirations(&self, expirations: &[ChainEpoch]) -> Result<(), String> {
        Ok(())
    }
    // TODO
    /// Adds sectors numbers to faults and fault epochs.
    pub fn add_faults<BS: BlockStore>(
        &self,
        store: &BS,
        sector_nos: &BitField,
        fault_epoch: &ChainEpoch,
    ) -> Result<(), String> {
        Ok(())
    }
    // TODO - actor error where its called
    /// Removes sector numbers from faults and fault epochs, if present.
    pub fn remove_faults<BS: BlockStore>(
        &self,
        store: &BS,
        sectors_nos: &BitField,
    ) -> Result<(), ActorError> {
        Ok(())
    }
    // TODO
    /// Iterates faults by declaration epoch, in order.
    fn for_each_fault_epoch<BS: BlockStore>(&self, store: &BS) -> Result<(), String> {
        Ok(())
    }
    // TODO
    fn clear_fault_epochs<BS: BlockStore>(
        &self,
        store: &BS,
        epochs: &[ChainEpoch],
    ) -> Result<(), String> {
        Ok(())
    }
    // TODO
    /// Adds sectors to recoveries.
    fn add_recoveries(&self, sector_nos: BitField) -> Result<(), String> {
        Ok(())
    }
    // TODO
    /// Removes sectors from recoveries, if present.
    pub fn remove_recoveries(&self, sector_nos: &BitField) -> Result<(), ActorError> {
        Ok(())
    }
    // TODO
    /// Loads sector info for a sequence of sectors.
    pub fn load_sector_infos<BS: BlockStore>(
        &self,
        store: &BS,
        sectors: BitField,
    ) -> Result<Vec<SectorOnChainInfo>, String> {
        todo!()
    }
    // TODO
    /// Loads info for a set of sectors to be proven.
    /// If any of the sectors are declared faulty and not to be recovered, info for the first non-faulty sector is substituted instead.
    /// If any of the sectors are declared recovered, they are returned from this method.
    pub fn load_sector_infos_for_proof<BS: BlockStore>(
        &self,
        store: &BS,
        proven_sectors: BitField,
    ) -> Result<(Vec<SectorOnChainInfo>, BitField), ActorError> {
        todo!()
    }
    // TODO
    /// Loads sector info for a sequence of sectors, substituting info for a stand-in sector for any that are faulty.
    fn load_sector_infos_with_fault_mask<BS: BlockStore>(
        &self,
        store: &BS,
        sectors: BitField,
        faults: BitField,
        fault_stand_in: SectorNumber,
    ) -> Result<Vec<SectorOnChainInfo>, String> {
        todo!()
    }
    // TODO
    /// Adds partition numbers to the set of PoSt submissions
    fn add_post_submissions(&self, partitions_nos: BitField) -> Result<(), String> {
        Ok(())
    }
    // TODO
    /// Removes all PoSt submissions
    pub fn clear_post_submissions(&mut self) -> Result<(), String> {
        self.post_submissions = BitField::default();
        Ok(())
    }
    // TODO
    // NOTE: ActorError needs to be returned; exitcode.ErrIllegalState, "failed to load deadlines"
    pub fn load_deadlines<BS: BlockStore>(&self, store: &BS) -> Result<Deadlines, String> {
        if let Some(deadlines) = store
            .get::<Deadlines>(&self.deadlines)
            .map_err(|e| e.to_string())?
        {
            Ok(deadlines)
        } else {
            Err(format!(
                    "load deadlines err: {}",
                    self.deadlines
                ))
        }
    }
    // TODO
    pub fn save_deadlines<BS: BlockStore>(
        &mut self,
        store: &BS,
        deadlines: Deadlines,
    ) -> Result<(), String> {
        let c = store.put(&deadlines, Blake2b256).map_err(|e| e.to_string())?;
        self.deadlines = c;
        Ok(())
    }

    //
    // Funds and vesting
    //

    pub fn add_pre_commit_deposit(&mut self, amount: &TokenAmount) {
        self.pre_commit_deposit += amount
    }

    pub fn add_locked_funds<BS: BlockStore>(
        &mut self,
        store: &BS,
        current_epoch: &ChainEpoch,
        vesting_sum: &TokenAmount,
        spec: VestSpec,
    ) -> Result<(), AmtError> {
        let mut vesting_funds: Amt<u64, _> = Amt::load(&self.vesting_funds, store)?;

        // Nothing unlocks here, this is just the start of the clock
        let vest_begin = current_epoch + spec.initial_delay;
        let vest_period = BigUint::from(spec.vest_period);

        let mut vested_so_far = BigUint::zero();
        let e = vest_begin + spec.step_duration;

        while &vested_so_far < vesting_sum {
            let vest_epoch = quantize_up(e, spec.quantization);
            let elapsed = vest_epoch - vest_begin;

            let mut target_vest = BigUint::zero();
            if elapsed < spec.vest_period {
                // Linear vesting, PARAM_FINISH
                target_vest = &(vesting_sum * elapsed) / &vest_period;
            } else {
                target_vest = vesting_sum.clone();
            }

            let vest_this_time = &target_vest - vested_so_far;
            vested_so_far = target_vest;

            // Load existing entry, else set a new one
            // TODO ask about biguint ser here and whether this should all be BigInt
            if let Some(locked_fund_entry) = vesting_funds.get(vest_epoch)? {
                let mut locked_funds = BigUint::from(locked_fund_entry);
                locked_funds += vest_this_time;
                let num = ToPrimitive::to_u64(&locked_funds).ok_or("something").unwrap();
                vesting_funds.set(vest_epoch, num);
            }
        }
        self.vesting_funds = vesting_funds.flush()?;
        self.locked_funds += vesting_sum;
        Ok(())
    }

    fn unlock_unvested_funds<BS: BlockStore>(
        &self,
        store: &BS,
        current_epoch: ChainEpoch,
        target: TokenAmount,
    ) -> Result<TokenAmount, String> {
        todo!()
    }

    /// Unlocks all vesting funds that have vested before the provided epoch.
    /// Returns the amount unlocked.
    pub fn unlock_vested_funds<BS: BlockStore>(
        &self,
        store: &BS,
        current_epoch: ChainEpoch,
    ) -> Result<TokenAmount, String> {
        todo!()
    }

    /// CheckVestedFunds returns the amount of vested funds that have vested before the provided epoch.
    fn check_vested_funds<BS: BlockStore>(
        &self,
        store: &BS,
        current_epoch: ChainEpoch,
    ) -> Result<TokenAmount, String> {
        todo!()
    }

    pub fn get_available_balance(&self, actor_balance: &TokenAmount) -> TokenAmount {
        (actor_balance - &self.locked_funds) - &self.pre_commit_deposit
    }

    pub fn assert_balance_invariants(&self, balance: &TokenAmount) {
        assert!(balance > &(&self.pre_commit_deposit + &self.locked_funds))
    }

    // pub fn get_storage_weight_desc_for_sector<BS: BlockStore>(
    //     &self,
    //     store: &BS,
    //     sector_num: SectorNumber,
    // ) -> Result<power::SectorStorageWeightDesc, String> {
    //     let sector_info = self
    //         .get_sector(store, sector_num)?
    //         .ok_or(format!("no such sector {}", sector_num))?;

    //     Ok(as_storage_weight_desc(self.info.sector_size, sector_info))
    // }
    // pub fn in_challenge_window<BS, RT>(&self, epoch: ChainEpoch) -> bool
    // where
    //     BS: BlockStore,
    //     RT: Runtime<BS>,
    // {
    //     // TODO revisit TODO in spec impl
    //     match *self.post_state.proving_period_start {
    //         Some(e) => epoch > e,
    //         None => true,
    //     }
    // }
    // pub fn compute_proving_set<BS: BlockStore>(
    //     &self,
    //     store: &BS,
    // ) -> Result<Vec<SectorInfo>, String> {
    //     let proving_set = Amt::<SectorOnChainInfo, _>::load(&self.sectors, store)?;

    //     let max_allowed_faults = self.get_max_allowed_faults(store)?;
    //     if self.fault_set.count_ones() > max_allowed_faults as usize {
    //         return Err("Bitfield larger than maximum allowed".to_owned());
    //     }

    //     let mut sector_infos: Vec<SectorInfo> = Vec::new();
    //     proving_set.for_each(|i, v: &SectorOnChainInfo| {
    //         if *v.declared_fault_epoch != None || *v.declared_fault_duration != None {
    //             return Err("sector fault epoch or duration invalid".to_owned());
    //         }

    //         let fault = match self.fault_set.get(i as usize) {
    //             Some(true) => true,
    //             _ => false,
    //         };
    //         if !fault {
    //             sector_infos.push(SectorInfo {
    //                 sealed_cid: v.info.sealed_cid.clone(),
    //                 sector_number: v.info.sector_number,
    //                 proof: v.info.registered_proof,
    //             });
    //         }
    //         Ok(())
    //     })?;

    //     Ok(sector_infos)
    // }
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

    /// The proof type used by this miner for sealing sectors.
    pub seal_proof_type: RegisteredProof,

    /// Amount of space in each sector committed to the network by this miner
    pub sector_size: SectorSize,

    /// The number of sectors in each Window PoSt partition (proof).
    /// This is computed from the proof type and represented here redundantly.
    pub window_post_partition_sectors: u64,
}

#[derive(Debug, PartialEq, Clone, Serialize_tuple, Deserialize_tuple)]
pub struct SectorOnChainInfo {
    pub info: SectorPreCommitInfo,

    /// Epoch at which SectorProveCommit is accepted
    pub activation_epoch: ChainEpoch,

    /// Integral of active deals over sector lifetime, 0 if CommittedCapacity sector
    #[serde(with = "bigint_ser")]
    pub deal_weight: BigInt,

    /// Integral of active verified deals over sector lifetime
    #[serde(with = "bigint_ser")]
    pub verified_deal_weight: BigInt,
}

impl SectorOnChainInfo {
    fn new(
        info: SectorPreCommitInfo,
        activation_epoch: ChainEpoch,
        deal_weight: BigInt,
        verified_deal_weight: BigInt,
    ) -> Self {
        Self {
            info,
            activation_epoch,
            deal_weight,
            verified_deal_weight,
        }
    }
    fn as_sector_info(&self) -> SectorInfo {
        SectorInfo {
            proof: self.info.registered_proof,
            sector_number: self.info.sector_number,
            sealed_cid: self.info.sealed_cid.clone(),
        }
    }
}

pub fn as_storage_weight_desc(
    sector_size: &SectorSize,
    sector_info: &SectorOnChainInfo,
) -> power::SectorStorageWeightDesc {
    power::SectorStorageWeightDesc {
        sector_size: *sector_size,
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
    // A bitfield of sector numbers due at each deadline.
    // The sectors for each deadline are logically grouped into sequential partitions for proving.
    // TODO should have capacity of WPOST_PERIOD_DEADLINES
    pub due: BitField,
}

impl Deadlines {
    pub fn new() -> Self {
        let d: BitVec<Lsb0, u8> = BitVec::with_capacity(WPOST_PERIOD_DEADLINES as usize);
        Self { due: d }
    }

    fn add_to_deadline(&self, deadline: u64, new_sectors: &[u64]) -> Result<(), String> {
        Ok(())
    }

    fn remove_from_all_deadlines(&self, sector_nos: BitField) -> Result<(), String> {
        Ok(())
    }
}

//
// Misc helpers
//

// fn delete_many(amt: Amt, keys: &[u64]) -> Result<(), AmtError> {
//     for i in keys {
//         amt.delete(i)?;
//     }
//     Ok(())
// }

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
            sector_size: SectorSize::_2KiB,
            seal_proof_type: RegisteredProof::default(),
            window_post_partition_sectors: 0,
        };
        let bz = to_vec(&info).unwrap();
        assert_eq!(from_slice::<MinerInfo>(&bz).unwrap(), info);
    }
}
