// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::OptionalEpoch;
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use crypto::Signature;
use num_bigint::biguint_ser::{BigUintDe, BigUintSer};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::{PaddedPieceSize, TokenAmount};

/// Note: Deal Collateral is only released and returned to clients and miners
/// when the storage deal stops counting towards power. In the current iteration,
/// it will be released when the sector containing the storage deals expires,
/// even though some storage deals can expire earlier than the sector does.
/// Collaterals are denominated in PerEpoch to incur a cost for self dealing or
/// minimal deals that last for a long time.
/// Note: ClientCollateralPerEpoch may not be needed and removed pending future confirmation.
/// There will be a Minimum value for both client and provider deal collateral.
#[derive(Clone, Debug, PartialEq)]
pub struct DealProposal {
    pub piece_cid: Cid,
    pub piece_size: PaddedPieceSize,
    pub client: Address,
    pub provider: Address,

    pub start_epoch: ChainEpoch,
    pub end_epoch: ChainEpoch,
    pub storage_price_per_epoch: TokenAmount,

    pub provider_collateral: TokenAmount,
    pub client_collateral: TokenAmount,
}

impl DealProposal {
    pub fn duration(&self) -> ChainEpoch {
        self.end_epoch - self.start_epoch
    }
    pub fn total_storage_fee(&self) -> TokenAmount {
        self.storage_price_per_epoch.clone() * self.duration()
    }
    pub fn client_balance_requirement(&self) -> TokenAmount {
        self.client_collateral.clone() + self.total_storage_fee()
    }
    pub fn provider_balance_requirement(&self) -> &TokenAmount {
        &self.provider_collateral
    }
}

impl Serialize for DealProposal {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.piece_cid,
            &self.piece_size,
            &self.client,
            &self.provider,
            &self.start_epoch,
            &self.end_epoch,
            BigUintSer(&self.storage_price_per_epoch),
            BigUintSer(&self.provider_collateral),
            BigUintSer(&self.client_collateral),
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DealProposal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (
            piece_cid,
            piece_size,
            client,
            provider,
            start_epoch,
            end_epoch,
            BigUintDe(storage_price_per_epoch),
            BigUintDe(provider_collateral),
            BigUintDe(client_collateral),
        ) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            piece_cid,
            piece_size,
            client,
            provider,
            start_epoch,
            end_epoch,
            storage_price_per_epoch,
            provider_collateral,
            client_collateral,
        })
    }
}

/// ClientDealProposal is a DealProposal signed by a client
#[derive(Clone, Debug, PartialEq)]
pub struct ClientDealProposal {
    pub proposal: DealProposal,
    pub client_signature: Signature,
}

impl Serialize for ClientDealProposal {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.proposal, &self.client_signature).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ClientDealProposal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (proposal, client_signature) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            proposal,
            client_signature,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Copy)]
pub struct DealState {
    pub sector_start_epoch: OptionalEpoch,
    pub last_updated_epoch: OptionalEpoch,
    pub slash_epoch: OptionalEpoch,
}

impl Serialize for DealState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.sector_start_epoch,
            &self.last_updated_epoch,
            &self.slash_epoch,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DealState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (sector_start_epoch, last_updated_epoch, slash_epoch) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            sector_start_epoch,
            last_updated_epoch,
            slash_epoch,
        })
    }
}
