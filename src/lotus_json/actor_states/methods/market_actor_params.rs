// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::deal::DealID;
use crate::shim::econ::TokenAmount;
use crate::shim::piece::PaddedPieceSize;
use crate::shim::sector::RegisteredSealProof;
use crate::test_snapshots;
use fil_actors_shared::fvm_ipld_bitfield::BitField;

use ::cid::Cid;
use jsonrpsee::core::Serialize;
use paste::paste;
use schemars::JsonSchema;
use serde::Deserialize;
use std::fmt::Debug;

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct WithdrawBalanceParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json", rename = "ProviderOrClientAddress")]
    pub provider_or_client: Address,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount: TokenAmount,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AddBalanceParamsLotusJson(
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    Address,
);

macro_rules! impl_lotus_json_for_add_balance_params {
    ($type_suffix:path: $($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::AddBalanceParams {
                    type LotusJson = AddBalanceParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                serde_json::json!("f0100"),
                                Self { provider_or_client: $type_suffix::Address::new_id(100) }
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        AddBalanceParamsLotusJson(self.provider_or_client.into())
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            provider_or_client: json.0.into(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_add_balance_params!(fvm_shared2::address: 8, 9);
impl_lotus_json_for_add_balance_params!(fvm_shared3::address: 10, 11);
impl_lotus_json_for_add_balance_params!(fvm_shared4::address: 12, 13, 14, 15, 16);

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

impl_lotus_json_for_withdraw_balance_params!(8, 9, 10, 11, 12, 13, 14, 15, 16);

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

impl_lotus_json_for_label!(8, 9, 10, 11, 12, 13, 14, 15, 16);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DealProposalLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    #[serde(rename = "PieceCID")]
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
    pub label: LabelLotusJson,
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
                            label: label.into_lotus_json(),
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
                            label: fil_actor_market_state::[<v $version>]::Label::from_lotus_json(label), // delegate
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

impl_lotus_json_for_deal_proposal!(8, 9, 10, 11, 12, 13, 14, 15, 16);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ClientDealProposalV2LotusJson {
    pub proposal: DealProposalLotusJson,
    #[schemars(with = "LotusJson<fvm_shared2::crypto::signature::Signature>")]
    #[serde(with = "crate::lotus_json")]
    pub client_signature: fvm_shared2::crypto::signature::Signature,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ClientDealProposalV3LotusJson {
    pub proposal: DealProposalLotusJson,
    #[schemars(with = "LotusJson<fvm_shared3::crypto::signature::Signature>")]
    #[serde(with = "crate::lotus_json")]
    pub client_signature: fvm_shared3::crypto::signature::Signature,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ClientDealProposalV4LotusJson {
    pub proposal: DealProposalLotusJson,
    #[schemars(with = "LotusJson<fvm_shared4::crypto::signature::Signature>")]
    #[serde(with = "crate::lotus_json")]
    pub client_signature: fvm_shared4::crypto::signature::Signature,
}

macro_rules! impl_lotus_json_for_client_deal_proposal {
    ($type_suffix:path: $lotus_json_type:ty: $($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::ClientDealProposal {
                    type LotusJson = $lotus_json_type;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            proposal: self.proposal.into_lotus_json(),
                            client_signature: self.client_signature.into(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            proposal: fil_actor_market_state::[<v $version>]::DealProposal::from_lotus_json(json.proposal),
                            client_signature: json.client_signature.into(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_client_deal_proposal!(fvm_shared2::crypto::signature: ClientDealProposalV2LotusJson: 8, 9);
impl_lotus_json_for_client_deal_proposal!(fvm_shared3::crypto::signature: ClientDealProposalV3LotusJson: 10, 11);
impl_lotus_json_for_client_deal_proposal!(fvm_shared4::crypto::signature: ClientDealProposalV4LotusJson: 12, 13, 14, 15, 16);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PublishStorageDealsParamsV2LotusJson {
    pub deals: Vec<ClientDealProposalV2LotusJson>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PublishStorageDealsParamsV3LotusJson {
    pub deals: Vec<ClientDealProposalV3LotusJson>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PublishStorageDealsParamsV4LotusJson {
    pub deals: Vec<ClientDealProposalV4LotusJson>,
}

macro_rules! impl_lotus_json_for_publish_storage_deals_params {
    ($lotus_json_type:ty: $($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::PublishStorageDealsParams {
                    type LotusJson = $lotus_json_type;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            deals: self.deals.into_iter().map(|d| d.into_lotus_json()).collect(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            deals: json.deals.into_iter()
                            .map(|d| fil_actor_market_state::[<v $version>]::ClientDealProposal::from_lotus_json(d)) // delegate
                            .collect(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_publish_storage_deals_params!(PublishStorageDealsParamsV2LotusJson: 8, 9);
impl_lotus_json_for_publish_storage_deals_params!(PublishStorageDealsParamsV3LotusJson: 10, 11);
impl_lotus_json_for_publish_storage_deals_params!(PublishStorageDealsParamsV4LotusJson: 12, 13, 14, 15, 16);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SectorDealsLotusJson {
    #[schemars(with = "LotusJson<RegisteredSealProof>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "crate::lotus_json")]
    pub sector_type: Option<RegisteredSealProof>,
    pub sector_expiry: ChainEpoch,
    #[schemars(with = "LotusJson<DealID>")]
    #[serde(with = "crate::lotus_json", rename = "DealIDs")]
    pub deal_ids: Vec<DealID>,
}

macro_rules! impl_lotus_json_for_sector_deals {
    // Handling version where both `sector_number` and `sector_type` should be None (v8)
    ($type_suffix:path: no_sector_type: $($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::SectorDeals {
                    type LotusJson = SectorDealsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            sector_type: None,
                            sector_expiry: self.sector_expiry.into_lotus_json(),
                            deal_ids: self.deal_ids.into(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            sector_expiry: json.sector_expiry.into(),
                            deal_ids: json.deal_ids.into(),
                        }
                    }
                }
            }
        )+
    };
    // Handling versions where `sector_number` should be None (v9, v10, v11, v12)
    ($type_suffix:path: $($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::SectorDeals {
                    type LotusJson = SectorDealsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            sector_type: Some(self.sector_type.into()),
                            sector_expiry: self.sector_expiry.into_lotus_json(),
                            deal_ids: self.deal_ids.into(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            sector_expiry: json.sector_expiry.into(),
                            sector_type: json.sector_type.unwrap_or(RegisteredSealProof::invalid()).into(),
                            deal_ids: json.deal_ids.into(),
                        }
                    }
                }
            }
        )+
    };
    ($type_suffix:path: $($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::SectorDeals {
                    type LotusJson = SectorDealsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            sector_type: Some(self.sector_type.into()),
                            sector_expiry: self.sector_expiry.into_lotus_json(),
                            deal_ids: self.deal_ids.into(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            sector_type: json.sector_type.unwrap_or(RegisteredSealProof::invalid()).into(),
                            sector_expiry: json.sector_expiry.into(),
                            deal_ids: json.deal_ids.into(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_sector_deals!(fvm_shared2::sector: no_sector_type: 8);
impl_lotus_json_for_sector_deals!(fvm_shared3::sector: 9, 10, 11, 12);
impl_lotus_json_for_sector_deals!(fvm_shared4::sector: 13, 14, 15, 16);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct VerifyDealsForActivationParamsLotusJson {
    pub sectors: Vec<SectorDealsLotusJson>,
}

macro_rules! impl_lotus_json_for_publish_storage_deals_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::VerifyDealsForActivationParams {
                    type LotusJson = VerifyDealsForActivationParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            sectors: self.sectors.into_iter().map(|s| s.into_lotus_json()).collect(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            sectors: json
                                .sectors
                                .into_iter()
                                .map(|s| fil_actor_market_state::[<v $version>]::SectorDeals::from_lotus_json(s)) // delegate
                                .collect(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_publish_storage_deals_params!(8, 9, 10, 11, 12, 13, 14, 15, 16);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ActivateDealsParamsLotusJson {
    #[schemars(with = "LotusJson<DealID>")]
    #[serde(with = "crate::lotus_json")]
    pub deal_ids: Vec<DealID>,
    pub sector_expiry: ChainEpoch,
}

macro_rules! impl_lotus_json_for_activate_deals_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::ActivateDealsParams {
                    type LotusJson = ActivateDealsParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            deal_ids: self.deal_ids.into(),
                            sector_expiry: self.sector_expiry.into_lotus_json(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            deal_ids: json.deal_ids.into(),
                            sector_expiry: json.sector_expiry.into(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_activate_deals_params!(8, 9, 10, 11);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct BatchActivateDealsParamsLotusJson {
    pub sectors: Vec<SectorDealsLotusJson>,
    pub compute_cid: bool,
}

macro_rules! impl_lotus_json_for_batch_activate_deals_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::BatchActivateDealsParams {
                    type LotusJson = BatchActivateDealsParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            sectors: self.sectors.into_iter().map(|s| s.into_lotus_json()).collect(),
                            compute_cid: self.compute_cid,
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            sectors: json
                                .sectors
                                .into_iter()
                                .map(|s| fil_actor_market_state::[<v $version>]::SectorDeals::from_lotus_json(s)) // delegate
                                .collect(),
                            compute_cid: json.compute_cid,
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_batch_activate_deals_params!(12, 13, 14, 15, 16);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct OnMinerSectorsTerminateParamsLotusJsonV8 {
    pub epoch: ChainEpoch,
    #[schemars(with = "LotusJson<DealID>")]
    #[serde(with = "crate::lotus_json")]
    pub deal_ids: Vec<DealID>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct OnMinerSectorsTerminateParamsLotusJsonV13 {
    pub epoch: ChainEpoch,
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    pub sectors: BitField,
}

macro_rules! impl_lotus_json_for_on_miner_sectors_terminate_params {
    (OnMinerSectorsTerminateParamsLotusJsonV8: $($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::OnMinerSectorsTerminateParams {
                    type LotusJson = OnMinerSectorsTerminateParamsLotusJsonV8;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            epoch: self.epoch.into(),
                            deal_ids: self.deal_ids.into(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            epoch: json.epoch.into(),
                            deal_ids: json.deal_ids.into(),
                        }
                    }
                }
            }
        )+
    };
    (OnMinerSectorsTerminateParamsLotusJsonV13: $($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::OnMinerSectorsTerminateParams {
                    type LotusJson = OnMinerSectorsTerminateParamsLotusJsonV13;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            epoch: self.epoch.into(),
                            sectors: self.sectors.into(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            epoch: json.epoch.into(),
                            sectors: json.sectors.into(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_on_miner_sectors_terminate_params!(OnMinerSectorsTerminateParamsLotusJsonV8: 8, 9, 10, 11, 12);
impl_lotus_json_for_on_miner_sectors_terminate_params!(OnMinerSectorsTerminateParamsLotusJsonV13: 13, 14, 15, 16);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct SectorDataSpecLotusJson {
    #[schemars(with = "LotusJson<DealID>")]
    #[serde(with = "crate::lotus_json")]
    pub deal_ids: Vec<DealID>,
    #[schemars(with = "LotusJson<RegisteredSealProof>")]
    #[serde(with = "crate::lotus_json")]
    pub sector_type: RegisteredSealProof,
}

macro_rules! impl_lotus_json_for_sector_data_spec {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::SectorDataSpec {
                    type LotusJson = SectorDataSpecLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            deal_ids: self.deal_ids.into(),
                            sector_type: self.sector_type.into(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            deal_ids: json.deal_ids.into(),
                            sector_type: json.sector_type.into(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_sector_data_spec!(8, 9, 10, 11);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ComputeDataCommitmentParamsLotusJson {
    pub inputs: Vec<SectorDataSpecLotusJson>,
}

macro_rules! impl_lotus_json_for_compute_data_commitment_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::ComputeDataCommitmentParams {
                    type LotusJson = ComputeDataCommitmentParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                           inputs: self.inputs.into_iter().map(|s| s.into_lotus_json()).collect(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            inputs: json
                                .inputs
                                .into_iter()
                                .map(|s| fil_actor_market_state::[<v $version>]::SectorDataSpec::from_lotus_json(s)) // delegate
                                .collect(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_compute_data_commitment_params!(8, 9, 10, 11);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DealQueryParamsLotusJson(
    #[schemars(with = "LotusJson<DealID>")]
    #[serde(with = "crate::lotus_json")]
    DealID,
);

macro_rules! impl_lotus_json_for_deal_query_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::DealQueryParams {
                    type LotusJson = DealQueryParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        DealQueryParamsLotusJson(self.id.into())
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            id: json.0.into(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_deal_query_params!(10, 11, 12, 13, 14, 15, 16);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SettleDealPaymentsParamsLotusJson(
    #[schemars(with = "LotusJson<BitField>")]
    #[serde(with = "crate::lotus_json")]
    BitField,
);

macro_rules! impl_lotus_json_for_settle_deal_payments_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_market_state::[<v $version>]::SettleDealPaymentsParams {
                    type LotusJson = SettleDealPaymentsParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        SettleDealPaymentsParamsLotusJson(self.deal_ids.into())
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            deal_ids: json.0.into(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_settle_deal_payments_params!(13, 14, 15, 16);

test_snapshots!(fil_actor_market_state: AddBalanceParams: 8, 9, 10, 11, 12, 13, 14, 15, 16);
