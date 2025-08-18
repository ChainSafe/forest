// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use crate::shim::piece::PaddedPieceSize;

use ::cid::Cid;
use fil_actor_market_state::v16::{DealProposal, Label};
use schemars::JsonSchema;
use serde::Deserialize;
use std::fmt::Debug;

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DealProposalLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    #[serde(rename = "CodeCID")]
    pub piece_cid: Cid,
    #[schemars(with = "LotusJson<PaddedPieceSize>")]
    #[serde(with = "crate::lotus_json")]
    pub piece_size: PaddedPieceSize,
    pub verified_deal: bool,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub client: Address,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub provider: Address,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub label: Label,
    pub start_epoch: ChainEpoch,
    pub end_epoch: ChainEpoch,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub storage_price_per_epoch: TokenAmount,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub provider_collateral: TokenAmount,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub client_collateral: TokenAmount,
}

impl HasLotusJson for DealProposal {
    type LotusJson = DealProposalLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let Self {
            piece_cid,
            piece_size,
            verified_deal,
            client,
            provider,
            label,
            start_epoch,
            end_epoch,
            storage_price_per_epoch,
            provider_collateral,
            client_collateral,
        } = self;
        Self::LotusJson {
            piece_cid: piece_cid.into(),
            piece_size: piece_size.into(),
            verified_deal: verified_deal.into(),
            client: client.into(),
            provider: provider.into(),
            label: label.into(),
            start_epoch: start_epoch.into(),
            end_epoch: end_epoch.into(),
            storage_price_per_epoch: storage_price_per_epoch.into(),
            provider_collateral: provider_collateral.into(),
            client_collateral: client_collateral.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            piece_cid,
            piece_size,
            verified_deal,
            client,
            provider,
            label,
            start_epoch,
            end_epoch,
            storage_price_per_epoch,
            provider_collateral,
            client_collateral,
        } = lotus_json;
        Self {
            piece_cid,
            piece_size: piece_size.into(),
            verified_deal,
            client: client.into(),
            provider: provider.into(),
            label,
            start_epoch,
            end_epoch,
            storage_price_per_epoch: storage_price_per_epoch.into(),
            provider_collateral: provider_collateral.into(),
            client_collateral: client_collateral.into(),
        }
    }
}
