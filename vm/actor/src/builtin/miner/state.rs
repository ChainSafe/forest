// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{power, u64_key, BytesKey, OptionalEpoch, HAMT_BIT_WIDTH};
use address::Address;
use bitfield::BitField;
use cid::Cid;
use clock::ChainEpoch;
use encoding::{serde_bytes, tuple::*};
use fil_types::{RegisteredProof, SectorInfo, SectorNumber, SectorSize};
use ipld_amt::{Amt, Error as AmtError};
use ipld_blockstore::BlockStore;
use ipld_hamt::{Error as HamtError, Hamt};
use num_bigint::bigint_ser;
use num_bigint::biguint_ser;
use num_bigint::BigInt;
use runtime::Runtime;
use vm::{DealID, TokenAmount};

/// Miner actor state
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct State {
    /// Map, HAMT<SectorNumber, SectorPreCommitOnChainInfo>
    pub pre_committed_sectors: Cid,

    /// Sectors this miner has committed
    /// Array, AMT<SectorOnChainInfo>
    pub sectors: Cid,

    /// BitField of faults
    pub fault_set: BitField,

    /// Sectors in proving set
    /// Array, AMT<SectorOnChainInfo>
    pub proving_set: Cid,

    /// Contains static info about this miner
    // TODO revisit as will likely change to Cid in future
    pub info: MinerInfo,

    /// The height at which this miner was slashed at.
    /// Array, AMT<SectorOnChainInfo>
    pub post_state: PoStState,
}

impl State {
    pub fn new(
        empty_arr: Cid,
        empty_map: Cid,
        owner: Address,
        worker: Address,
        peer_id: Vec<u8>,
        sector_size: SectorSize,
    ) -> Self {
        Self {
            pre_committed_sectors: empty_map,
            sectors: empty_arr.clone(),
            fault_set: BitField::default(),
            proving_set: empty_arr,
            info: MinerInfo {
                owner,
                worker,
                pending_worker_key: None,
                peer_id,
                sector_size,
            },
            post_state: PoStState {
                proving_period_start: OptionalEpoch(None),
                num_consecutive_failures: 0,
            },
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
    pub fn get_storage_weight_desc_for_sector<BS: BlockStore>(
        &self,
        store: &BS,
        sector_num: SectorNumber,
    ) -> Result<power::SectorStorageWeightDesc, String> {
        let sector_info = self
            .get_sector(store, sector_num)?
            .ok_or(format!("no such sector {}", sector_num))?;

        Ok(as_storage_weight_desc(self.info.sector_size, sector_info))
    }
    pub fn in_challenge_window<BS, RT>(&self, epoch: ChainEpoch) -> bool
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO revisit TODO in spec impl
        match *self.post_state.proving_period_start {
            Some(e) => epoch > e,
            None => true,
        }
    }
    pub fn compute_proving_set<BS: BlockStore>(
        &mut self,
        store: &BS,
    ) -> Result<Vec<SectorInfo>, String> {
        let proving_set = Amt::<SectorOnChainInfo, _>::load(&self.sectors, store)?;

        let max_allowed_faults = self.get_max_allowed_faults(store)?;
        if self.fault_set.count()? > max_allowed_faults as usize {
            return Err("Bitfield larger than maximum allowed".to_owned());
        }

        let mut sector_infos: Vec<SectorInfo> = Vec::new();
        proving_set.for_each(|i, v: &SectorOnChainInfo| {
            if *v.declared_fault_epoch != None || *v.declared_fault_duration != None {
                return Err("sector fault epoch or duration invalid".to_owned());
            }

            let fault = self.fault_set.get(i)?;
            if !fault {
                sector_infos.push(SectorInfo {
                    sealed_cid: v.info.sealed_cid.clone(),
                    sector_number: v.info.sector_number,
                    proof: v.info.registered_proof,
                });
            }
            Ok(())
        })?;

        Ok(sector_infos)
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

    /// Amount of space in each sector committed to the network by this miner
    pub sector_size: SectorSize,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct PoStState {
    /// Epoch that starts the current proving period
    pub proving_period_start: OptionalEpoch,

    /// Number of surprised post challenges that have been failed since last successful PoSt.
    /// Indicates that the claimed storage power may not actually be proven. Recovery can proceed by
    /// submitting a correct response to a subsequent PoSt challenge, up until
    /// the limit of number of consecutive failures.
    pub num_consecutive_failures: i64,
}

impl PoStState {
    pub fn has_failed_post(&self) -> bool {
        self.num_consecutive_failures > 0
    }
    pub fn is_ok(&self) -> bool {
        !self.has_failed_post()
    }
}

#[derive(Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct WorkerKeyChange {
    /// Must be an ID address
    pub new_worker: Address,
    pub effective_at: ChainEpoch,
}

#[derive(Debug, PartialEq, Clone, Serialize_tuple, Deserialize_tuple)]
pub struct SectorPreCommitInfo {
    pub registered_proof: RegisteredProof,
    pub sector_number: SectorNumber,
    pub sealed_cid: Cid,
    pub seal_rand_epoch: ChainEpoch,
    pub deal_ids: Vec<DealID>,
    pub expiration: ChainEpoch,
}

#[derive(Debug, PartialEq, Clone, Serialize_tuple, Deserialize_tuple)]
pub struct SectorPreCommitOnChainInfo {
    pub info: SectorPreCommitInfo,
    #[serde(with = "biguint_ser")]
    pub pre_commit_deposit: TokenAmount,
    pub pre_commit_epoch: ChainEpoch,
}

#[derive(Debug, PartialEq, Clone, Serialize_tuple, Deserialize_tuple)]
pub struct SectorOnChainInfo {
    pub info: SectorPreCommitInfo,

    /// Epoch at which SectorProveCommit is accepted
    pub activation_epoch: ChainEpoch,

    /// Integral of active deals over sector lifetime, 0 if CommittedCapacity sector
    #[serde(with = "bigint_ser")]
    pub deal_weight: BigInt,

    /// Fixed pledge collateral requirement determined at activation
    #[serde(with = "biguint_ser")]
    pub pledge_requirement: TokenAmount,

    /// Can be undefined
    pub declared_fault_epoch: OptionalEpoch,

    pub declared_fault_duration: OptionalEpoch,
}

fn as_storage_weight_desc(
    sector_size: SectorSize,
    sector_info: SectorOnChainInfo,
) -> power::SectorStorageWeightDesc {
    power::SectorStorageWeightDesc {
        sector_size,
        deal_weight: sector_info.deal_weight,
        duration: sector_info.info.expiration - sector_info.activation_epoch,
    }
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
        };
        let bz = to_vec(&info).unwrap();
        assert_eq!(from_slice::<MinerInfo>(&bz).unwrap(), info);
    }
}
