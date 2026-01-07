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
use pastey::paste;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
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
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_add_balance_params_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::AddBalanceParams;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = AddBalanceParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    serde_json::json!("f0100"),
                                    Self { provider_or_client: Address::new_id(100).into() }
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
            }
        )+
    };
}

macro_rules! impl_lotus_json_for_withdraw_balance_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_withdraw_balance_params_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::WithdrawBalanceParams;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = WithdrawBalanceParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                 json!({
                                        "ProviderOrClientAddress": "f01234",
                                        "Amount": "1000000000000000000",
                                    }),
                                Self{
                                     provider_or_client: Address::new_id(1234).into(),
                                     amount: TokenAmount::from_atto(1000000000000000000u64).into(),
                                 },
                            )]
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
            }
        )+
    };
}

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
                mod [<impl_label_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::Label;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = LabelLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (serde_json::json!("label-string"), Self::String("label-string".to_owned())),
                                (serde_json::json!([1,2,3]), Self::Bytes(vec![1,2,3])),
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
            }
        )+
    };
}

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
                mod [<impl_deal_proposal_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::DealProposal;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = DealProposalLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            // Create minimal test data using Default where possible

                            // Note: We need to create version-specific test data due to different fvm_shared versions
                            // For now, we'll use a minimal example that should work across versions
                            let test_cid = ::cid::Cid::try_from("baga6ea4seaqao7s73y24kcutaosvacpdjgfe5pw76ooefnyqw4ynr3d2y6x2mpq").unwrap();

                            vec![(
                                serde_json::json!({
                                    "PieceCID": { "/": test_cid.to_string() },
                                    "PieceSize": 1024,
                                    "VerifiedDeal": false,
                                    "Client": "f01234",
                                    "Provider": "f05678",
                                    "Label": "test",
                                    "StartEpoch": 100,
                                    "EndEpoch": 200,
                                    "StoragePricePerEpoch": "1000",
                                    "ProviderCollateral": "2000",
                                    "ClientCollateral": "3000"
                                }),
                                // Create the corresponding object using from_lotus_json to ensure compatibility
                                Self::from_lotus_json(DealProposalLotusJson {
                                    piece_cid: test_cid,
                                    piece_size: 1024u64.into(),
                                    verified_deal: false,
                                    client: Address::new_id(1234).into(),
                                    provider: Address::new_id(5678).into(),
                                    label: LabelLotusJson::String("test".to_string()),
                                    start_epoch: 100,
                                    end_epoch: 200,
                                    storage_price_per_epoch: TokenAmount::from_atto(1000u64).into(),
                                    provider_collateral: TokenAmount::from_atto(2000u64).into(),
                                    client_collateral: TokenAmount::from_atto(3000u64).into(),
                                })
                            )]
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
            }
        )+
    };
}

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
                mod [<impl_client_deal_proposal_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::ClientDealProposal;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = $lotus_json_type;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            // Use the same test data as DealProposal, but add a client signature
                            let test_cid = ::cid::Cid::try_from("baga6ea4seaqao7s73y24kcutaosvacpdjgfe5pw76ooefnyqw4ynr3d2y6x2mpq").unwrap();

                            vec![(
                                serde_json::json!({
                                    "Proposal": {
                                        "PieceCID": { "/": test_cid.to_string() },
                                        "PieceSize": 1024,
                                        "VerifiedDeal": false,
                                        "Client": "f01234",
                                        "Provider": "f05678",
                                        "Label": "test",
                                        "StartEpoch": 100,
                                        "EndEpoch": 200,
                                        "StoragePricePerEpoch": "1000",
                                        "ProviderCollateral": "2000",
                                        "ClientCollateral": "3000"
                                    },
                                    "ClientSignature": {
                                        "Type": 1,
                                        "Data": "dGVzdA=="  // base64 for "test"
                                    }
                                }),
                                // Create object using from_lotus_json to ensure compatibility
                                Self::from_lotus_json($lotus_json_type {
                                    proposal: DealProposalLotusJson {
                                        piece_cid: test_cid,
                                        piece_size: 1024u64.into(),
                                        verified_deal: false,
                                        client: crate::shim::address::Address::new_id(1234).into(),
                                        provider: crate::shim::address::Address::new_id(5678).into(),
                                        label: LabelLotusJson::String("test".to_string()),
                                        start_epoch: 100,
                                        end_epoch: 200,
                                        storage_price_per_epoch: crate::shim::econ::TokenAmount::from_atto(1000u64).into(),
                                        provider_collateral: crate::shim::econ::TokenAmount::from_atto(2000u64).into(),
                                        client_collateral: crate::shim::econ::TokenAmount::from_atto(3000u64).into(),
                                    },
                                    client_signature: $type_suffix::Signature {
                                        sig_type: $type_suffix::SignatureType::Secp256k1,
                                        bytes: b"test".to_vec(),
                                    },
                                })
                            )]
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
            }
        )+
    };
}

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

macro_rules! impl_publish_storage_deals_params_snapshots_v2 {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_publish_storage_deals_params_v2_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::PublishStorageDealsParams;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = PublishStorageDealsParamsV2LotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({
                                        "Deals": [
                                            {
                                                "Proposal": {
                                                    "PieceCID": {
                                                        "/": "baga6ea4seaqao7s73y24kcutaosvacpdjgfe5pw76ooefnyqw4ynr3d2y6x2mpq"
                                                    },
                                                    "PieceSize": 1024,
                                                    "VerifiedDeal": false,
                                                    "Client": "f17uoq6tp427uzv7fztkbsnn64iwotfrristwpryy",
                                                    "Provider": "f01000",
                                                    "Label": "test-deal",
                                                    "StartEpoch": 100,
                                                    "EndEpoch": 200,
                                                    "StoragePricePerEpoch": "1000",
                                                    "ProviderCollateral": "2000",
                                                    "ClientCollateral": "1500"
                                                },
                                                "ClientSignature": {
                                                    "Type": 1,
                                                    "Data": "VGVzdCBzaWduYXR1cmU="
                                                }
                                            }
                                        ]
                                    }),
                                    T::from_lotus_json(PublishStorageDealsParamsV2LotusJson {
                                        deals: vec![
                                            ClientDealProposalV2LotusJson {
                                                proposal: DealProposalLotusJson {
                                                    piece_cid: "baga6ea4seaqao7s73y24kcutaosvacpdjgfe5pw76ooefnyqw4ynr3d2y6x2mpq".parse().unwrap(),
                                                    piece_size: 1024u64.into(),
                                                    verified_deal: false,
                                                    client: "f17uoq6tp427uzv7fztkbsnn64iwotfrristwpryy".parse().unwrap(),
                                                    provider: "f01000".parse().unwrap(),
                                                    label: LabelLotusJson::String("test-deal".to_string()),
                                                    start_epoch: ChainEpoch::from(100),
                                                    end_epoch: ChainEpoch::from(200),
                                                    storage_price_per_epoch: TokenAmount::from_atto(1000u64),
                                                    provider_collateral: TokenAmount::from_atto(2000u64),
                                                    client_collateral: TokenAmount::from_atto(1500u64),
                                                },
                                                client_signature: fvm_shared2::crypto::signature::Signature {
                                                    sig_type: fvm_shared2::crypto::signature::SignatureType::Secp256k1,
                                                    bytes: b"Test signature".to_vec(),
                                                },
                                            }
                                        ]
                                    })
                                )
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
            }
        )+
    };
}

macro_rules! impl_publish_storage_deals_params_snapshots_v3 {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_publish_storage_deals_params_v3_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::PublishStorageDealsParams;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = PublishStorageDealsParamsV3LotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({
                                        "Deals": [
                                            {
                                                "Proposal": {
                                                    "PieceCID": {
                                                        "/": "baga6ea4seaqao7s73y24kcutaosvacpdjgfe5pw76ooefnyqw4ynr3d2y6x2mpq"
                                                    },
                                                    "PieceSize": 1024,
                                                    "VerifiedDeal": false,
                                                    "Client": "f17uoq6tp427uzv7fztkbsnn64iwotfrristwpryy",
                                                    "Provider": "f01000",
                                                    "Label": "test-deal",
                                                    "StartEpoch": 100,
                                                    "EndEpoch": 200,
                                                    "StoragePricePerEpoch": "1000",
                                                    "ProviderCollateral": "2000",
                                                    "ClientCollateral": "1500"
                                                },
                                                "ClientSignature": {
                                                    "Type": 1,
                                                    "Data": "VGVzdCBzaWduYXR1cmU="
                                                }
                                            }
                                        ]
                                    }),
                                    T::from_lotus_json(PublishStorageDealsParamsV3LotusJson {
                                        deals: vec![
                                            ClientDealProposalV3LotusJson {
                                                proposal: DealProposalLotusJson {
                                                    piece_cid: "baga6ea4seaqao7s73y24kcutaosvacpdjgfe5pw76ooefnyqw4ynr3d2y6x2mpq".parse().unwrap(),
                                                    piece_size: 1024u64.into(),
                                                    verified_deal: false,
                                                    client: "f17uoq6tp427uzv7fztkbsnn64iwotfrristwpryy".parse().unwrap(),
                                                    provider: "f01000".parse().unwrap(),
                                                    label: LabelLotusJson::String("test-deal".to_string()),
                                                    start_epoch: ChainEpoch::from(100),
                                                    end_epoch: ChainEpoch::from(200),
                                                    storage_price_per_epoch: TokenAmount::from_atto(1000u64),
                                                    provider_collateral: TokenAmount::from_atto(2000u64),
                                                    client_collateral: TokenAmount::from_atto(1500u64),
                                                },
                                                client_signature: fvm_shared3::crypto::signature::Signature {
                                                    sig_type: fvm_shared3::crypto::signature::SignatureType::Secp256k1,
                                                    bytes: b"Test signature".to_vec(),
                                                },
                                            }
                                        ]
                                    })
                                )
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
            }
        )+
    };
}

macro_rules! impl_publish_storage_deals_params_snapshots_v4 {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_publish_storage_deals_params_v4_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::PublishStorageDealsParams;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = PublishStorageDealsParamsV4LotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({
                                        "Deals": [
                                            {
                                                "Proposal": {
                                                    "PieceCID": {
                                                        "/": "baga6ea4seaqao7s73y24kcutaosvacpdjgfe5pw76ooefnyqw4ynr3d2y6x2mpq"
                                                    },
                                                    "PieceSize": 1024,
                                                    "VerifiedDeal": false,
                                                    "Client": "f17uoq6tp427uzv7fztkbsnn64iwotfrristwpryy",
                                                    "Provider": "f01000",
                                                    "Label": "test-deal",
                                                    "StartEpoch": 100,
                                                    "EndEpoch": 200,
                                                    "StoragePricePerEpoch": "1000",
                                                    "ProviderCollateral": "2000",
                                                    "ClientCollateral": "1500"
                                                },
                                                "ClientSignature": {
                                                    "Type": 1,
                                                    "Data": "VGVzdCBzaWduYXR1cmU="
                                                }
                                            }
                                        ]
                                    }),
                                    T::from_lotus_json(PublishStorageDealsParamsV4LotusJson {
                                        deals: vec![
                                            ClientDealProposalV4LotusJson {
                                                proposal: DealProposalLotusJson {
                                                    piece_cid: "baga6ea4seaqao7s73y24kcutaosvacpdjgfe5pw76ooefnyqw4ynr3d2y6x2mpq".parse().unwrap(),
                                                    piece_size: 1024u64.into(),
                                                    verified_deal: false,
                                                    client: "f17uoq6tp427uzv7fztkbsnn64iwotfrristwpryy".parse().unwrap(),
                                                    provider: "f01000".parse().unwrap(),
                                                    label: LabelLotusJson::String("test-deal".to_string()),
                                                    start_epoch: ChainEpoch::from(100),
                                                    end_epoch: ChainEpoch::from(200),
                                                    storage_price_per_epoch: TokenAmount::from_atto(1000u64),
                                                    provider_collateral: TokenAmount::from_atto(2000u64),
                                                    client_collateral: TokenAmount::from_atto(1500u64),
                                                },
                                                client_signature: fvm_shared4::crypto::signature::Signature {
                                                    sig_type: fvm_shared4::crypto::signature::SignatureType::Secp256k1,
                                                    bytes: b"Test signature".to_vec(),
                                                },
                                            }
                                        ]
                                    })
                                )
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
            }
        )+
    };
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SectorDealsLotusJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub sector_number: Option<u64>,
    #[schemars(with = "LotusJson<RegisteredSealProof>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(with = "crate::lotus_json")]
    #[serde(default)]
    pub sector_type: Option<RegisteredSealProof>,
    pub sector_expiry: ChainEpoch,
    #[schemars(with = "LotusJson<DealID>")]
    #[serde(with = "crate::lotus_json", rename = "DealIDs")]
    pub deal_ids: Vec<DealID>,
}

macro_rules! impl_lotus_json_for_sector_deals {
    // Handling version where both `sector_number` and `sector_type` should be None (v8)
    ($type_suffix:path: no_sector_type: no_sector_number: $($version:literal),+) => {
        $(
            paste! {
                mod [<impl_sector_deals_no_sector_type_no_sector_number_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::SectorDeals;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = SectorDealsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    serde_json::json!({
                                        "SectorExpiry": 1000,
                                        "DealIDs": [1,2,3]
                                    }),
                                    Self {
                                        sector_expiry: 1000,
                                        deal_ids: vec![1,2,3],
                                    }
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            Self::LotusJson {
                                sector_number: None,
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
            }
        )+
    };
    // Handling versions where `sector_number` should be None (v9, v10, v11, v12)
    ($type_suffix:path: no_sector_number: $($version:literal),+) => {
        $(
            paste! {
                mod [<impl_sector_deals_no_sector_number_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::SectorDeals;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = SectorDealsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    serde_json::json!({
                                        "SectorType": 1,
                                        "SectorExpiry": 1000,
                                        "DealIDs": [1,2,3]
                                    }),
                                    Self {
                                        sector_type: RegisteredSealProof::from(1).into(),
                                        sector_expiry: 1000,
                                        deal_ids: vec![1,2,3],
                                    }
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            Self::LotusJson {
                                sector_number: None,
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
            }
        )+
    };
    ($type_suffix:path: $($version:literal),+) => {
        $(
            paste! {
                mod [<impl_sector_deals_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::SectorDeals;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = SectorDealsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    serde_json::json!({
                                        "SectorNumber": 42,
                                        "SectorType": 1,
                                        "SectorExpiry": 1000,
                                        "DealIDs": [1,2,3]
                                    }),
                                    Self {
                                        sector_number: 42,
                                        sector_type: RegisteredSealProof::from(1).into(),
                                        sector_expiry: 1000,
                                        deal_ids: vec![1,2,3],
                                    }
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            Self::LotusJson {
                                sector_number: Some(self.sector_number),
                                sector_type: Some(self.sector_type.into()),
                                sector_expiry: self.sector_expiry.into_lotus_json(),
                                deal_ids: self.deal_ids.into(),
                            }
                        }

                        fn from_lotus_json(json: Self::LotusJson) -> Self {
                            Self {
                                sector_number: json.sector_number.unwrap_or(0),
                                sector_type: json.sector_type.unwrap_or(RegisteredSealProof::invalid()).into(),
                                sector_expiry: json.sector_expiry.into(),
                                deal_ids: json.deal_ids.into(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct VerifyDealsForActivationParamsLotusJson {
    pub sectors: Vec<SectorDealsLotusJson>,
}

macro_rules! impl_lotus_json_for_verify_deals_for_activation_params {
    // Version 8: SectorDeals has only sector_expiry and deal_ids
    (v8) => {
        paste! {
            mod impl_verify_deals_for_activation_params_v8 {
                use super::*;
                type T = fil_actor_market_state::v8::VerifyDealsForActivationParams;

                #[test]
                fn snapshots() {
                    crate::lotus_json::assert_all_snapshots::<T>();
                }

                impl HasLotusJson for T {
                    type LotusJson = VerifyDealsForActivationParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                serde_json::json!({
                                    "Sectors": [
                                        {
                                            "SectorExpiry": 1000,
                                            "DealIDs": [1,2,3]
                                        }
                                    ]
                                }),
                                Self {
                                    sectors: vec![
                                        fil_actor_market_state::v8::SectorDeals {
                                            sector_expiry: 1000,
                                            deal_ids: vec![1,2,3],
                                        }
                                    ],
                                }
                            ),
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
                                .map(|s| fil_actor_market_state::v8::SectorDeals::from_lotus_json(s))
                                .collect(),
                        }
                    }
                }
            }
        }
    };
    // Versions 9-12: SectorDeals has sector_type (which gets default value invalid() = 0)
    (v9_to_v12: $($version:literal),+) => {
        $(
            paste! {
                mod [<impl_verify_deals_for_activation_params_v9_to_v12_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::VerifyDealsForActivationParams;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = VerifyDealsForActivationParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    serde_json::json!({
                                        "Sectors": [
                                            {
                                                "SectorExpiry": 1000,
                                                "DealIDs": [1,2,3],
                                                "SectorType": 0
                                            }
                                        ]
                                    }),
                                    Self {
                                        sectors: vec![
                                            fil_actor_market_state::[<v $version>]::SectorDeals {
                                                sector_expiry: 1000,
                                                deal_ids: vec![1,2,3],
                                                sector_type: RegisteredSealProof::invalid().into(),
                                            }
                                        ],
                                    }
                                ),
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
                                    .map(|s| fil_actor_market_state::[<v $version>]::SectorDeals::from_lotus_json(s))
                                    .collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
    // Versions 13+: SectorDeals has both sector_type and sector_number (both get default values)
    (v13_plus: $($version:literal),+) => {
        $(
            paste! {
                mod [<impl_verify_deals_for_activation_params_v13_plus_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::VerifyDealsForActivationParams;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = VerifyDealsForActivationParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    serde_json::json!({
                                        "Sectors": [
                                            {
                                                "SectorExpiry": 1000,
                                                "DealIDs": [1,2,3],
                                                "SectorType": 0,
                                                "SectorNumber": 0
                                            }
                                        ]
                                    }),
                                    Self {
                                        sectors: vec![
                                            fil_actor_market_state::[<v $version>]::SectorDeals {
                                                sector_expiry: 1000,
                                                deal_ids: vec![1,2,3],
                                                sector_type: RegisteredSealProof::invalid().into(),
                                                sector_number: 0u64.into(),
                                            }
                                        ],
                                    }
                                ),
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
                                    .map(|s| fil_actor_market_state::[<v $version>]::SectorDeals::from_lotus_json(s))
                                    .collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ActivateDealsParamsLotusJson {
    #[schemars(with = "LotusJson<DealID>")]
    #[serde(with = "crate::lotus_json", rename = "DealIDs")]
    pub deal_ids: Vec<DealID>,
    pub sector_expiry: ChainEpoch,
}

macro_rules! impl_lotus_json_for_activate_deals_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_activate_deals_params_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::ActivateDealsParams;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = ActivateDealsParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    serde_json::json!({
                                        "DealIDs": [1,2,3],
                                        "SectorExpiry": 1000
                                    }),
                                    Self {
                                        deal_ids: vec![1,2,3],
                                        sector_expiry: 1000,
                                    }
                                ),
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
            }
        )+
    };
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct BatchActivateDealsParamsLotusJson {
    pub sectors: Vec<SectorDealsLotusJson>,
    pub compute_cid: bool,
}

macro_rules! impl_lotus_json_for_batch_activate_deals_params {
    // Version 12: SectorDeals has sector_type but no sector_number
    (v12: $($version:literal),+) => {
        $(
            paste! {
                mod [<impl_batch_activate_deals_params_v12_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::BatchActivateDealsParams;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = BatchActivateDealsParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    serde_json::json!({
                                        "Sectors": [
                                            {
                                                "SectorType": 1,
                                                "SectorExpiry": 1000,
                                                "DealIDs": [1,2,3]
                                            }
                                        ],
                                        "ComputeCid": true
                                    }),
                                    Self {
                                        sectors: vec![
                                            fil_actor_market_state::[<v $version>]::SectorDeals::from_lotus_json(
                                                SectorDealsLotusJson {
                                                    sector_number: None, // No sector_number in v12
                                                    sector_type: Some(RegisteredSealProof::from(1).into()),
                                                    sector_expiry: 1000,
                                                    deal_ids: vec![1,2,3],
                                                }
                                            )
                                        ],
                                        compute_cid: true,
                                    }
                                ),
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
                                    .map(|s| fil_actor_market_state::[<v $version>]::SectorDeals::from_lotus_json(s))
                                    .collect(),
                                compute_cid: json.compute_cid,
                            }
                        }
                    }
                }
            }
        )+
    };
    // Versions 13-16: SectorDeals has both sector_type and sector_number
    (v13_onwards: $($version:literal),+) => {
        $(
            paste! {
                mod [<impl_batch_activate_deals_params_v13_onwards_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::BatchActivateDealsParams;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = BatchActivateDealsParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    serde_json::json!({
                                        "Sectors": [
                                            {
                                                "SectorNumber": 42,
                                                "SectorType": 1,
                                                "SectorExpiry": 1000,
                                                "DealIDs": [1,2,3]
                                            }
                                        ],
                                        "ComputeCid": true
                                    }),
                                    Self {
                                        sectors: vec![
                                            fil_actor_market_state::[<v $version>]::SectorDeals::from_lotus_json(
                                                SectorDealsLotusJson {
                                                    sector_number: Some(42), // Has sector_number in v13+
                                                    sector_type: Some(RegisteredSealProof::from(1).into()),
                                                    sector_expiry: 1000,
                                                    deal_ids: vec![1,2,3],
                                                }
                                            )
                                        ],
                                        compute_cid: true,
                                    }
                                ),
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
                                    .map(|s| fil_actor_market_state::[<v $version>]::SectorDeals::from_lotus_json(s))
                                    .collect(),
                                compute_cid: json.compute_cid,
                            }
                        }
                    }
                }
            }
        )+
    };
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct OnMinerSectorsTerminateParamsLotusJsonV8 {
    pub epoch: ChainEpoch,
    #[schemars(with = "LotusJson<DealID>")]
    #[serde(with = "crate::lotus_json", rename = "DealIDs")]
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
                mod [<impl_on_miner_sectors_terminate_params_v8_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::OnMinerSectorsTerminateParams;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = OnMinerSectorsTerminateParamsLotusJsonV8;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    serde_json::json!({
                                        "Epoch": 1000,
                                        "DealIDs": [1,2,3]
                                    }),
                                    Self {
                                        epoch: 1000,
                                        deal_ids: vec![1,2,3],
                                    }
                                ),
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
            }
        )+
    };
    (OnMinerSectorsTerminateParamsLotusJsonV13: $($version:literal),+) => {
        $(
            paste! {
                mod [<impl_on_miner_sectors_terminate_params_v13_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::OnMinerSectorsTerminateParams;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = OnMinerSectorsTerminateParamsLotusJsonV13;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            let mut sectors = BitField::new();
                            sectors.set(1);

                            vec![
                                (
                                    serde_json::json!({
                                        "Epoch": 1000,
                                        "Sectors": [1, 1]
                                    }),
                                    Self {
                                        epoch: 1000,
                                        sectors,
                                    }
                                ),
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
            }
        )+
    };
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SectorDataSpecLotusJson {
    #[schemars(with = "LotusJson<DealID>")]
    #[serde(with = "crate::lotus_json", rename = "DealIDs")]
    pub deal_ids: Vec<DealID>,
    #[schemars(with = "LotusJson<RegisteredSealProof>")]
    #[serde(with = "crate::lotus_json")]
    pub sector_type: RegisteredSealProof,
}

macro_rules! impl_lotus_json_for_sector_data_spec {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_sector_data_spec_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::SectorDataSpec;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = SectorDataSpecLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    serde_json::json!({
                                        "DealIDs": [1,2,3],
                                        "SectorType": 1
                                    }),
                                    Self {
                                        deal_ids: vec![1,2,3],
                                        sector_type: RegisteredSealProof::from(1).into(),
                                    }
                                ),
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
            }
        )+
    };
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ComputeDataCommitmentParamsLotusJson {
    pub inputs: Vec<SectorDataSpecLotusJson>,
}

macro_rules! impl_lotus_json_for_compute_data_commitment_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_compute_data_commitment_params_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::ComputeDataCommitmentParams;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = ComputeDataCommitmentParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    serde_json::json!({
                                        "Inputs": [
                                            {
                                                "DealIDs": [1,2,3],
                                                "SectorType": 1
                                            }
                                        ]
                                    }),
                                    Self {
                                        inputs: vec![
                                            fil_actor_market_state::[<v $version>]::SectorDataSpec::from_lotus_json(
                                                SectorDataSpecLotusJson {
                                                    deal_ids: vec![1,2,3],
                                                    sector_type: RegisteredSealProof::from(1).into(),
                                                }
                                            )
                                        ],
                                    }
                                ),
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
            }
        )+
    };
}

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
                mod [<impl_deal_query_params_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::DealQueryParams;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = DealQueryParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    serde_json::json!(42),
                                    Self { id: 42 }
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            DealQueryParamsLotusJson(self.id.into())
                        }

                        fn from_lotus_json(json: Self::LotusJson) -> Self {
                            Self {
                                id: json.0,
                            }
                        }
                    }
                }
            }
        )+
    };
}

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
                mod [<impl_settle_deal_payments_params_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::SettleDealPaymentsParams;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = SettleDealPaymentsParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    serde_json::json!([0]),
                                    Self { deal_ids: BitField::new() }
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            SettleDealPaymentsParamsLotusJson(self.deal_ids.into())
                        }

                        fn from_lotus_json(json: Self::LotusJson) -> Self {
                            Self {
                                deal_ids: json.0,
                            }
                        }
                    }
                }
            }
        )+
    };
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PieceChangeLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub data: Cid,
    #[schemars(with = "LotusJson<PaddedPieceSize>")]
    #[serde(with = "crate::lotus_json")]
    pub size: PaddedPieceSize,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    pub payload: Vec<u8>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SectorChangesLotusJson {
    pub sector: u64,
    pub minimum_commitment_epoch: ChainEpoch,
    pub added: Vec<PieceChangeLotusJson>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SectorContentChangedParamsLotusJson {
    pub sectors: Vec<SectorChangesLotusJson>,
}

macro_rules! impl_lotus_json_for_sector_content_changed_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_sector_content_changed_params_ $version>] {
                    use super::*;
                    type T = fil_actor_market_state::[<v $version>]::ext::miner::SectorContentChangedParams;

                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = SectorContentChangedParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    serde_json::json!({
                                        "Sectors": []
                                    }),
                                    Self {
                                        sectors: vec![],
                                    }
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            Self::LotusJson {
                                sectors: self.sectors.into_iter().map(|sector_changes| {
                                    SectorChangesLotusJson {
                                        sector: sector_changes.sector.into(),
                                        minimum_commitment_epoch: sector_changes.minimum_commitment_epoch,
                                        added: sector_changes.added.into_iter().map(|piece_change| {
                                            PieceChangeLotusJson {
                                                data: piece_change.data.into(),
                                                size: piece_change.size.into(),
                                                payload: piece_change.payload.into(),
                                            }
                                        }).collect(),
                                    }
                                }).collect(),
                            }
                        }

                        fn from_lotus_json(json: Self::LotusJson) -> Self {
                            Self {
                                sectors: json.sectors.into_iter().map(|sector_changes_json| {
                                    fil_actor_market_state::[<v $version>]::ext::miner::SectorChanges {
                                        sector: sector_changes_json.sector.into(),
                                        minimum_commitment_epoch: sector_changes_json.minimum_commitment_epoch,
                                        added: sector_changes_json.added.into_iter().map(|piece_change_json| {
                                            fil_actor_market_state::[<v $version>]::ext::miner::PieceChange {
                                                data: piece_change_json.data.into(),
                                                size: piece_change_json.size.into(),
                                                payload: piece_change_json.payload.into(),
                                            }
                                        }).collect(),
                                    }
                                }).collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_add_balance_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_withdraw_balance_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_label!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_deal_proposal!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_client_deal_proposal!(fvm_shared2::crypto::signature: ClientDealProposalV2LotusJson: 8, 9);
impl_lotus_json_for_client_deal_proposal!(fvm_shared3::crypto::signature: ClientDealProposalV3LotusJson: 10, 11);
impl_lotus_json_for_client_deal_proposal!(fvm_shared4::crypto::signature: ClientDealProposalV4LotusJson: 12, 13, 14, 15, 16, 17);
impl_publish_storage_deals_params_snapshots_v2!(8, 9);
impl_publish_storage_deals_params_snapshots_v3!(10, 11);
impl_publish_storage_deals_params_snapshots_v4!(12, 13, 14, 15, 16, 17);
impl_lotus_json_for_sector_deals!(fvm_shared2::sector: no_sector_type: no_sector_number: 8);
impl_lotus_json_for_sector_deals!(fvm_shared3::sector: no_sector_number: 9, 10, 11, 12);
impl_lotus_json_for_sector_deals!(fvm_shared4::sector: 13, 14, 15, 16, 17);
impl_lotus_json_for_verify_deals_for_activation_params!(v8);
impl_lotus_json_for_verify_deals_for_activation_params!(v9_to_v12: 9, 10, 11, 12);
impl_lotus_json_for_verify_deals_for_activation_params!(v13_plus: 13, 14, 15, 16, 17);
impl_lotus_json_for_activate_deals_params!(8, 9, 10, 11);
impl_lotus_json_for_batch_activate_deals_params!(v12: 12);
impl_lotus_json_for_batch_activate_deals_params!(v13_onwards: 13, 14, 15, 16, 17);
impl_lotus_json_for_on_miner_sectors_terminate_params!(OnMinerSectorsTerminateParamsLotusJsonV8: 8, 9, 10, 11, 12);
impl_lotus_json_for_on_miner_sectors_terminate_params!(OnMinerSectorsTerminateParamsLotusJsonV13: 13, 14, 15, 16, 17);
impl_lotus_json_for_sector_data_spec!(8, 9, 10, 11);
impl_lotus_json_for_compute_data_commitment_params!(8, 9, 10, 11);
impl_lotus_json_for_deal_query_params!(10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_settle_deal_payments_params!(13, 14, 15, 16, 17);
impl_lotus_json_for_sector_content_changed_params!(13, 14, 15, 16, 17);

// Tests for GetDeal*Params types (type aliases for DealQueryParams)
test_snapshots!(fil_actor_market_state: GetDealActivationParams: 10, 11, 12, 13, 14, 15, 16, 17);
test_snapshots!(fil_actor_market_state: GetDealClientCollateralParams: 10, 11, 12, 13, 14, 15, 16, 17);
test_snapshots!(fil_actor_market_state: GetDealClientParams: 10, 11, 12, 13, 14, 15, 16, 17);
test_snapshots!(fil_actor_market_state: GetDealDataCommitmentParams: 10, 11, 12, 13, 14, 15, 16, 17);
test_snapshots!(fil_actor_market_state: GetDealLabelParams: 10, 11, 12, 13, 14, 15, 16, 17);
test_snapshots!(fil_actor_market_state: GetDealProviderCollateralParams: 10, 11, 12, 13, 14, 15, 16, 17);
test_snapshots!(fil_actor_market_state: GetDealProviderParams: 10, 11, 12, 13, 14, 15, 16, 17);
test_snapshots!(fil_actor_market_state: GetDealTermParams: 10, 11, 12, 13, 14, 15, 16, 17);
test_snapshots!(fil_actor_market_state: GetDealTotalPriceParams: 10, 11, 12, 13, 14, 15, 16, 17);
test_snapshots!(fil_actor_market_state: GetDealVerifiedParams: 10, 11, 12, 13, 14, 15, 16, 17);
test_snapshots!(fil_actor_market_state: GetDealSectorParams: 13, 14, 15, 16, 17);
