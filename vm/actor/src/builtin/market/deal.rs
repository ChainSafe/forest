// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::OptionalEpoch;
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use crypto::Signature;
use encoding::tuple::*;
use encoding::Cbor;
use fil_types::PaddedPieceSize;
use num_bigint::biguint_ser;
use vm::TokenAmount;

/// Note: Deal Collateral is only released and returned to clients and miners
/// when the storage deal stops counting towards power. In the current iteration,
/// it will be released when the sector containing the storage deals expires,
/// even though some storage deals can expire earlier than the sector does.
/// Collaterals are denominated in PerEpoch to incur a cost for self dealing or
/// minimal deals that last for a long time.
/// Note: ClientCollateralPerEpoch may not be needed and removed pending future confirmation.
/// There will be a Minimum value for both client and provider deal collateral.
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct DealProposal {
    pub piece_cid: Cid,
    pub piece_size: PaddedPieceSize,
    pub verified_deal: bool,
    pub client: Address,
    pub provider: Address,

    // Nominal start epoch. Deal payment is linear between StartEpoch and EndEpoch,
    // with total amount StoragePricePerEpoch * (EndEpoch - StartEpoch).
    // Storage deal must appear in a sealed (proven) sector no later than StartEpoch,
    // otherwise it is invalid.
    pub start_epoch: ChainEpoch,
    pub end_epoch: ChainEpoch,
    #[serde(with = "biguint_ser")]
    pub storage_price_per_epoch: TokenAmount,

    #[serde(with = "biguint_ser")]
    pub provider_collateral: TokenAmount,
    #[serde(with = "biguint_ser")]
    pub client_collateral: TokenAmount,
}

impl Cbor for DealProposal {}

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

/// ClientDealProposal is a DealProposal signed by a client
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct ClientDealProposal {
    pub proposal: DealProposal,
    pub client_signature: Signature,
}

#[derive(Clone, Debug, PartialEq, Copy, Serialize_tuple, Deserialize_tuple)]
pub struct DealState {
    pub sector_start_epoch: OptionalEpoch,
    pub last_updated_epoch: OptionalEpoch,
    pub slash_epoch: OptionalEpoch,
}
