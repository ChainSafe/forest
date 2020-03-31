// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::DealWeight;
use address::Address;
use clock::ChainEpoch;
use encoding::Cbor;
use num_bigint::bigint_ser::{BigIntDe, BigIntSer};
use num_bigint::biguint_ser::{BigUintDe, BigUintSer};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::{SectorSize, Serialized, TokenAmount};

pub type SectorTermination = i64;

/// Implicit termination after all deals expire
pub const SECTOR_TERMINATION_EXPIRED: SectorTermination = 0;
/// Unscheduled explicit termination by the miner
pub const SECTOR_TERMINATION_MANUAL: SectorTermination = 1;

#[derive(Clone)]
pub struct SectorStorageWeightDesc {
    pub sector_size: SectorSize,
    pub duration: ChainEpoch,
    pub deal_weight: DealWeight,
}

impl Serialize for SectorStorageWeightDesc {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.sector_size,
            &self.duration,
            BigIntSer(&self.deal_weight),
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SectorStorageWeightDesc {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (sector_size, duration, BigIntDe(deal_weight)) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            sector_size,
            duration,
            deal_weight,
        })
    }
}

pub struct AddBalanceParams {
    pub miner: Address,
}

impl Serialize for AddBalanceParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [&self.miner].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AddBalanceParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [miner]: [Address; 1] = Deserialize::deserialize(deserializer)?;
        Ok(Self { miner })
    }
}

pub struct WithdrawBalanceParams {
    pub miner: Address,
    pub requested: TokenAmount,
}

impl Serialize for WithdrawBalanceParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.miner, BigUintSer(&self.requested)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WithdrawBalanceParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (miner, BigUintDe(requested)) = Deserialize::deserialize(deserializer)?;
        Ok(Self { miner, requested })
    }
}

// TODO on miner impl, alias these params for constructor
pub struct CreateMinerParams {
    pub owner_addr: Address,
    pub worker_addr: Address,
    pub sector_size: SectorSize,
    pub peer: String,
}

impl Cbor for CreateMinerParams {}
impl Serialize for CreateMinerParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.owner_addr,
            &self.worker_addr,
            &self.sector_size,
            &self.peer,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CreateMinerParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (owner_addr, worker_addr, sector_size, peer) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            owner_addr,
            worker_addr,
            sector_size,
            peer,
        })
    }
}

pub struct CreateMinerReturn {
    /// Canonical ID-based address for the actor.
    pub id_address: Address,
    /// Re-org safe address for created actor
    pub robust_address: Address,
}

impl Serialize for CreateMinerReturn {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.id_address, &self.robust_address).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CreateMinerReturn {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (id_address, robust_address) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            id_address,
            robust_address,
        })
    }
}

pub struct DeleteMinerParams {
    pub miner: Address,
}

impl Serialize for DeleteMinerParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [&self.miner].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DeleteMinerParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [miner]: [Address; 1] = Deserialize::deserialize(deserializer)?;
        Ok(Self { miner })
    }
}

pub struct OnSectorProveCommitParams {
    pub weight: SectorStorageWeightDesc,
}

impl Serialize for OnSectorProveCommitParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [&self.weight].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OnSectorProveCommitParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [weight]: [SectorStorageWeightDesc; 1] = Deserialize::deserialize(deserializer)?;
        Ok(Self { weight })
    }
}

pub struct OnSectorTerminateParams {
    pub termination_type: SectorTermination,
    pub weights: Vec<SectorStorageWeightDesc>,
    pub pledge: TokenAmount,
}

impl Serialize for OnSectorTerminateParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.termination_type,
            &self.weights,
            BigUintSer(&self.pledge),
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OnSectorTerminateParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (termination_type, weights, BigUintDe(pledge)) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            termination_type,
            weights,
            pledge,
        })
    }
}

pub struct OnSectorTemporaryFaultEffectiveBeginParams {
    // TODO revisit todo for replacing with power
    pub weights: Vec<SectorStorageWeightDesc>,
    pub pledge: TokenAmount,
}

impl Serialize for OnSectorTemporaryFaultEffectiveBeginParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.weights, BigUintSer(&self.pledge)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OnSectorTemporaryFaultEffectiveBeginParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (weights, BigUintDe(pledge)) = Deserialize::deserialize(deserializer)?;
        Ok(Self { weights, pledge })
    }
}

pub struct OnSectorTemporaryFaultEffectiveEndParams {
    // TODO revisit todo for replacing with power
    pub weights: Vec<SectorStorageWeightDesc>,
    pub pledge: TokenAmount,
}

impl Serialize for OnSectorTemporaryFaultEffectiveEndParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.weights, BigUintSer(&self.pledge)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OnSectorTemporaryFaultEffectiveEndParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (weights, BigUintDe(pledge)) = Deserialize::deserialize(deserializer)?;
        Ok(Self { weights, pledge })
    }
}

pub struct OnSectorModifyWeightDescParams {
    // TODO revisit todo in spec to change with power
    pub prev_weight: SectorStorageWeightDesc,
    pub prev_pledge: TokenAmount,
    pub new_weight: SectorStorageWeightDesc,
}

impl Serialize for OnSectorModifyWeightDescParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.prev_weight,
            BigUintSer(&self.prev_pledge),
            &self.new_weight,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OnSectorModifyWeightDescParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (prev_weight, BigUintDe(prev_pledge), new_weight) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            prev_weight,
            prev_pledge,
            new_weight,
        })
    }
}

pub struct OnMinerWindowedPoStFailureParams {
    pub num_consecutive_failures: i64,
}

impl Serialize for OnMinerWindowedPoStFailureParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [&self.num_consecutive_failures].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OnMinerWindowedPoStFailureParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [num_consecutive_failures]: [i64; 1] = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            num_consecutive_failures,
        })
    }
}

pub struct EnrollCronEventParams {
    pub event_epoch: ChainEpoch,
    pub payload: Serialized,
}

impl Serialize for EnrollCronEventParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.event_epoch, &self.payload).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EnrollCronEventParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (event_epoch, payload) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            event_epoch,
            payload,
        })
    }
}

pub struct ReportConsensusFaultParams {
    pub block_header_1: Serialized,
    pub block_header_2: Serialized,
    pub block_header_extra: Serialized,
}

impl Serialize for ReportConsensusFaultParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.block_header_1,
            &self.block_header_2,
            &self.block_header_extra,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ReportConsensusFaultParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (block_header_1, block_header_2, block_header_extra) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            block_header_1,
            block_header_2,
            block_header_extra,
        })
    }
}
