// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use crate::shim::fvm_shared_latest::crypto::signature::Signature;
use crate::shim::piece::PaddedPieceSize;
use fil_actor_market_state::v16::{ClientDealProposal, DealProposal, Label};

use ::cid::Cid;
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

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum LabelLotusJson {
    String(String),
    Bytes(Vec<u8>),
}

macro_rules! impl_lotus_json_for_label {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::Label {
                    type LotusJson = LabelLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        match self {
                            Self::Bytes(bytes) => LabelLotusJson::Bytes(bytes),
                            Self::String(string) => LabelLotusJson::String(string),
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        match lotus_json {
                            LabelLotusJson::Bytes(bytes) => Self::Bytes(bytes),
                            LabelLotusJson::String(string) => Self::String(string),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_label!(15, 16);

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
    // #[schemars(with = "LotusJson<Label>")]
    // #[serde(with = "crate::lotus_json")]
    // pub label: Label,
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

macro_rules! impl_lotus_json_for_deal_proposal {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::DealProposal {
                    type LotusJson = DealProposalLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
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
                            // label: label.into(),
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
                            // label,
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
                            label: todo!(),
                            start_epoch,
                            end_epoch,
                            storage_price_per_epoch: storage_price_per_epoch.into(),
                            provider_collateral: provider_collateral.into(),
                            client_collateral: client_collateral.into(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_deal_proposal!(15, 16);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ClientDealProposalLotusJson {
    #[schemars(with = "LotusJson<DealProposal>")]
    #[serde(with = "crate::lotus_json")]
    pub proposal: DealProposal,
    // #[schemars(with = "LotusJson<Signature>")]
    // #[serde(with = "crate::lotus_json")]
    // pub client_signature: Signature,
}

macro_rules! impl_lotus_json_for_client_deal_proposal {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::ClientDealProposal {
                    type LotusJson = ClientDealProposalLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            proposal: todo!(), //self.proposal.into(),
                            // TODO
                            // client_signature: self.client_signature.into(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            proposal: todo!(), //json.proposal.into(),
                            // TODO
                            client_signature: Signature::new_bls(vec![]),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_client_deal_proposal!(15, 16);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PublishStorageDealsParamsLotusJson {
    #[schemars(with = "LotusJson<ClientDealProposal>")]
    #[serde(with = "crate::lotus_json")]
    pub deals: Vec<ClientDealProposal>,
}

macro_rules! impl_lotus_json_for_publish_storage_deals_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::PublishStorageDealsParams {
                    type LotusJson = PublishStorageDealsParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            deals: self.deals.into_iter().map(|deal| deal.into()).collect(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            deals: json.deals.into_iter().map(|deal| deal.into()).collect(),
                        }
                    }
                }
            }
        )+
    };
}

//impl_lotus_json_for_publish_storage_deals_params!(9, 10, 11, 12, 13, 14, 15, 16);
impl_lotus_json_for_publish_storage_deals_params!(15, 16);
