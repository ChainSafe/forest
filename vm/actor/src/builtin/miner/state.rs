// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::OptionalEpoch;
use ::serde::{Deserialize, Deserializer, Serialize, Serializer};
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use ipld_amt::{Amt, Error as AmtError};
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser::{BigIntDe, BigIntSer};
use num_bigint::biguint_ser::{BigUintDe, BigUintSer};
use num_bigint::BigInt;
use rleplus::bitvec::prelude::{BitVec, Lsb0};
use rleplus::{BitVecDe, BitVecSer};
use vm::{DealID, RegisteredProof, SectorNumber, SectorSize, TokenAmount};

/// Miner actor state
pub struct State {
    /// Map, HAMT<SectorNumber, SectorPreCommitOnChainInfo>
    pub pre_committed_sectors: Cid,

    /// Sectors this miner has committed
    /// Array, AMT<SectorOnChainInfo>
    pub sectors: Cid,

    /// BitField of faults
    pub fault_set: BitVec<Lsb0, u8>,

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
        peer_id: String,
        sector_size: SectorSize,
    ) -> Self {
        Self {
            pre_committed_sectors: empty_map,
            sectors: empty_arr.clone(),
            fault_set: BitVec::default(),
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
}

impl Serialize for State {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.pre_committed_sectors,
            &self.sectors,
            BitVecSer(&self.fault_set),
            &self.proving_set,
            &self.info,
            &self.post_state,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for State {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (pre_committed_sectors, sectors, BitVecDe(fault_set), proving_set, info, post_state) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            pre_committed_sectors,
            sectors,
            fault_set,
            proving_set,
            info,
            post_state,
        })
    }
}

/// Static information about miner
#[derive(Debug, PartialEq)]
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
    // TODO revisit this, broken because invalid utf8 bytes will panic
    pub peer_id: String,

    /// Amount of space in each sector committed to the network by this miner
    pub sector_size: SectorSize,
}

impl Serialize for MinerInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.owner,
            &self.worker,
            &self.pending_worker_key,
            &self.peer_id,
            &self.sector_size,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MinerInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (owner, worker, pending_worker_key, peer_id, sector_size) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            owner,
            worker,
            pending_worker_key,
            peer_id,
            sector_size,
        })
    }
}

pub struct PoStState {
    /// Epoch that starts the current proving period
    pub proving_period_start: OptionalEpoch,

    /// Number of surprised post challenges that have been failed since last successful PoSt.
    /// Indicates that the claimed storage power may not actually be proven. Recovery can proceed by
    /// submitting a correct response to a subsequent PoSt challenge, up until
    /// the limit of number of consecutive failures.
    pub num_consecutive_failures: i64,
}

impl Serialize for PoStState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.proving_period_start, &self.num_consecutive_failures).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PoStState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (proving_period_start, num_consecutive_failures) =
            Deserialize::deserialize(deserializer)?;

        Ok(Self {
            proving_period_start,
            num_consecutive_failures,
        })
    }
}

#[derive(Debug, PartialEq)]
pub struct WorkerKeyChange {
    /// Must be an ID address
    pub new_worker: Address,
    pub effective_at: ChainEpoch,
}

impl Serialize for WorkerKeyChange {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.new_worker, &self.effective_at).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WorkerKeyChange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (new_worker, effective_at) = Deserialize::deserialize(deserializer)?;

        Ok(Self {
            new_worker,
            effective_at,
        })
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct SectorPreCommitInfo {
    pub registered_proof: RegisteredProof,
    pub sector_number: SectorNumber,
    pub sealed_cid: Cid,
    pub seal_rand_epoch: ChainEpoch,
    pub deal_ids: Vec<DealID>,
    pub expiration: ChainEpoch,
}

impl Serialize for SectorPreCommitInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.registered_proof,
            &self.sector_number,
            &self.sealed_cid,
            &self.seal_rand_epoch,
            &self.deal_ids,
            &self.expiration,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SectorPreCommitInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (registered_proof, sector_number, sealed_cid, seal_rand_epoch, deal_ids, expiration) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            registered_proof,
            sector_number,
            sealed_cid,
            seal_rand_epoch,
            deal_ids,
            expiration,
        })
    }
}

#[derive(Debug, PartialEq)]
pub struct SectorPreCommitOnChainInfo {
    pub info: SectorPreCommitInfo,
    pub pre_commit_deposit: TokenAmount,
    pub pre_commit_epoch: ChainEpoch,
}

impl Serialize for SectorPreCommitOnChainInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.info,
            BigUintSer(&self.pre_commit_deposit),
            &self.pre_commit_epoch,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SectorPreCommitOnChainInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (info, BigUintDe(pre_commit_deposit), pre_commit_epoch) =
            Deserialize::deserialize(deserializer)?;

        Ok(Self {
            info,
            pre_commit_deposit,
            pre_commit_epoch,
        })
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct SectorOnChainInfo {
    pub info: SectorPreCommitInfo,

    /// Epoch at which SectorProveCommit is accepted
    pub activation_epoch: ChainEpoch,

    /// Integral of active deals over sector lifetime, 0 if CommittedCapacity sector
    pub deal_weight: BigInt,

    /// Fixed pledge collateral requirement determined at activation
    pub pledge_requirement: TokenAmount,

    /// Can be undefined
    pub declared_fault_epoch: OptionalEpoch,

    pub declared_fault_duration: OptionalEpoch,
}

impl Serialize for SectorOnChainInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.info,
            &self.activation_epoch,
            BigIntSer(&self.deal_weight),
            BigUintSer(&self.pledge_requirement),
            &self.declared_fault_epoch,
            &self.declared_fault_duration,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SectorOnChainInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (
            info,
            activation_epoch,
            BigIntDe(deal_weight),
            BigUintDe(pledge_requirement),
            declared_fault_epoch,
            declared_fault_duration,
        ) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            info,
            activation_epoch,
            deal_weight,
            pledge_requirement,
            declared_fault_epoch,
            declared_fault_duration,
        })
    }
}
