// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use crate::shim::piece::PaddedPieceSize;

use ::cid::Cid;
// use fil_actor_market_state::v16::ClientDealProposal;
use fil_actor_market_state::v16::Label;
// use fvm_ipld_encoding::RawBytes;
use jsonrpsee::core::Serialize;
use paste::paste;
use schemars::JsonSchema;
use serde::Deserialize;
use std::fmt::Debug;

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct WithdrawBalanceParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub provider_or_client: Address,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount: TokenAmount,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AddBalanceParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub provider_or_client: Address,
}

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

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PublishStorageDealsParamsLotusJson {
    // #[schemars(with = "LotusJson<ClientDealProposal>")]
    // #[serde(with = "crate::lotus_json")]
    // pub deals: Vec<ClientDealProposal>,
}

macro_rules! impl_lotus_json_for_add_balance_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::AddBalanceParams {
                    type LotusJson = AddBalanceParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            provider_or_client: self.provider_or_client.into(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            provider_or_client: json.provider_or_client.into(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_add_balance_params!(11, 12, 13, 14, 15, 16);

macro_rules! impl_lotus_json_for_withdraw_balance_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::WithdrawBalanceParams {
                    type LotusJson = WithdrawBalanceParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            provider_or_client: self.provider_or_client.into(),
                            amount: self.amount.into(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            provider_or_client: json.provider_or_client.into(),
                            amount: json.amount.into(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_withdraw_balance_params!(9, 10, 11, 12, 13, 14, 15, 16);

//PublishStorageDealsParams
// macro_rules! impl_lotus_json_for_publish_storage_deals_params {
//     ($($version:literal),+) => {
//         $(
//             paste! {
//                 impl HasLotusJson for fil_actor_market_state::[<v $version>]::PublishStorageDealsParams {
//                     type LotusJson = PublishStorageDealsParamsLotusJson;

//                     #[cfg(test)]
//                     fn snapshots() -> Vec<(serde_json::Value, Self)> {
//                         vec![
//                         ]
//                     }

//                     fn into_lotus_json(self) -> Self::LotusJson {
//                         Self::LotusJson {
//                             deals: self.deals.into(),
//                         }
//                     }

//                     fn from_lotus_json(json: Self::LotusJson) -> Self {
//                         Self {
//                             deals: json.deals.into(),
//                         }
//                     }
//                 }
//             }
//         )+
//     };
// }

// impl_lotus_json_for_publish_storage_deals_params!(9, 10, 11, 12, 13, 14, 15, 16);
