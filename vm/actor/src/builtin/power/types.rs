// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{DealWeight, StoragePower};
use address::Address;
use clock::ChainEpoch;
use num_bigint::{
    bigint_ser::{BigIntDe, BigIntSer},
    BigInt,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::SectorSize;

lazy_static! {
    /// Minimum number of registered miners for the minimum miner size limit to effectively limit consensus power.
    pub static ref CONSENSUS_MINER_MIN_POWER: StoragePower = BigInt::from(2 << 30);
}

/// Storage miner actor constructor params are defined here so the power actor can send them
/// to the init actor to instantiate miners.
pub struct MinerConstructorParams {
    pub owner_addr: Address,
    pub worker_addr: Address,
    pub sector_size: SectorSize,
    pub peer_id: String,
}

impl Serialize for MinerConstructorParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.owner_addr,
            &self.worker_addr,
            &self.sector_size,
            &self.peer_id,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MinerConstructorParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (owner_addr, worker_addr, sector_size, peer_id) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            owner_addr,
            worker_addr,
            sector_size,
            peer_id,
        })
    }
}

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
