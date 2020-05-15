// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::deal::ClientDealProposal;
use address::Address;
use clock::ChainEpoch;
use fil_types::{RegisteredProof, SectorSize};
use num_bigint::biguint_ser::{BigUintDe, BigUintSer};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::{DealID, TokenAmount};

pub struct WithdrawBalanceParams {
    pub provider_or_client: Address,
    pub amount: TokenAmount,
}

impl Serialize for WithdrawBalanceParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.provider_or_client, BigUintSer(&self.amount)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WithdrawBalanceParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (provider_or_client, BigUintDe(amount)) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            provider_or_client,
            amount,
        })
    }
}

pub struct OnMinerSectorsTerminateParams {
    pub deal_ids: Vec<DealID>,
}

impl Serialize for OnMinerSectorsTerminateParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [&self.deal_ids].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OnMinerSectorsTerminateParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [deal_ids]: [Vec<DealID>; 1] = Deserialize::deserialize(deserializer)?;
        Ok(Self { deal_ids })
    }
}

pub struct HandleExpiredDealsParams {
    pub deal_ids: Vec<DealID>,
}

impl Serialize for HandleExpiredDealsParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [&self.deal_ids].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for HandleExpiredDealsParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [deal_ids]: [Vec<DealID>; 1] = Deserialize::deserialize(deserializer)?;
        Ok(Self { deal_ids })
    }
}

pub struct PublishStorageDealsParams {
    pub deals: Vec<ClientDealProposal>,
}

impl Serialize for PublishStorageDealsParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [&self.deals].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PublishStorageDealsParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [deals]: [Vec<ClientDealProposal>; 1] = Deserialize::deserialize(deserializer)?;
        Ok(Self { deals })
    }
}

pub struct PublishStorageDealsReturn {
    pub ids: Vec<DealID>,
}

impl Serialize for PublishStorageDealsReturn {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [&self.ids].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PublishStorageDealsReturn {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [ids]: [Vec<DealID>; 1] = Deserialize::deserialize(deserializer)?;
        Ok(Self { ids })
    }
}

pub struct VerifyDealsOnSectorProveCommitParams {
    pub deal_ids: Vec<DealID>,
    pub sector_size: SectorSize,
    pub sector_expiry: ChainEpoch,
}

impl Serialize for VerifyDealsOnSectorProveCommitParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.deal_ids, &self.sector_size, &self.sector_expiry).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for VerifyDealsOnSectorProveCommitParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (deal_ids, sector_size, sector_expiry) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            deal_ids,
            sector_size,
            sector_expiry,
        })
    }
}

pub struct ComputeDataCommitmentParams {
    pub deal_ids: Vec<DealID>,
    pub sector_type: RegisteredProof,
}

impl Serialize for ComputeDataCommitmentParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.deal_ids, &self.sector_type).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ComputeDataCommitmentParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (deal_ids, sector_type) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            deal_ids,
            sector_type,
        })
    }
}
