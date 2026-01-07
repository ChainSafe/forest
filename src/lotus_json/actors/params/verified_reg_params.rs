// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::sector::SectorNumber;
use ::cid::Cid;
use fvm_ipld_encoding::RawBytes;
use fvm_shared4::{ActorID, bigint::BigInt};
use pastey::paste;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ConstructorParamsLotusJson(
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    Address,
);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct VerifierParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub address: Address,
    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub allowance: BigInt,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoveVerifierParamsLotusJson(
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    Address,
);

// Version-specific structs for different FVM versions to avoid conversion issues
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoveDataCapParamsV2LotusJson {
    #[serde(with = "crate::lotus_json")]
    pub verified_client_to_remove: Address,
    #[serde(with = "crate::lotus_json")]
    pub data_cap_amount_to_remove: BigInt,
    pub verifier_request_1: RemoveDataCapRequestV2LotusJson,
    pub verifier_request_2: RemoveDataCapRequestV2LotusJson,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoveDataCapRequestV2LotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub verifier: Address,
    #[schemars(with = "LotusJson<fvm_shared2::crypto::signature::Signature>")]
    #[serde(with = "crate::lotus_json")]
    #[serde(rename = "VerifierSignature")]
    pub signature: fvm_shared2::crypto::signature::Signature,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoveDataCapParamsV3LotusJson {
    #[serde(with = "crate::lotus_json")]
    pub verified_client_to_remove: Address,
    #[serde(with = "crate::lotus_json")]
    pub data_cap_amount_to_remove: BigInt,
    pub verifier_request_1: RemoveDataCapRequestV3LotusJson,
    pub verifier_request_2: RemoveDataCapRequestV3LotusJson,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoveDataCapRequestV3LotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub verifier: Address,
    #[schemars(with = "LotusJson<fvm_shared3::crypto::signature::Signature>")]
    #[serde(with = "crate::lotus_json")]
    #[serde(rename = "VerifierSignature")]
    pub signature: fvm_shared3::crypto::signature::Signature,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoveDataCapParamsV4LotusJson {
    #[serde(with = "crate::lotus_json")]
    pub verified_client_to_remove: Address,
    #[serde(with = "crate::lotus_json")]
    pub data_cap_amount_to_remove: BigInt,
    pub verifier_request_1: RemoveDataCapRequestV4LotusJson,
    pub verifier_request_2: RemoveDataCapRequestV4LotusJson,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoveDataCapRequestV4LotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub verifier: Address,
    #[schemars(with = "LotusJson<fvm_shared4::crypto::signature::Signature>")]
    #[serde(with = "crate::lotus_json")]
    #[serde(rename = "VerifierSignature")]
    pub signature: fvm_shared4::crypto::signature::Signature,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoveExpiredAllocationsParamsLotusJson {
    #[schemars(with = "LotusJson<ActorID>")]
    #[serde(with = "crate::lotus_json")]
    pub client: ActorID,
    #[schemars(with = "LotusJson<Vec<u64>>")]
    #[serde(with = "crate::lotus_json")]
    pub allocation_ids: Vec<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ClaimAllocationsParamsLotusJson {
    pub sectors: Vec<SectorAllocationClaimsLotusJson>,
    pub all_or_nothing: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SectorAllocationClaimsLotusJson {
    #[serde(with = "crate::lotus_json")]
    pub sector: SectorNumber,
    #[serde(with = "crate::lotus_json")]
    #[serde(rename = "SectorExpiry")]
    pub expiry: ChainEpoch,
    pub claims: Vec<AllocationClaimLotusJson>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AllocationClaimLotusJson {
    #[serde(with = "crate::lotus_json")]
    pub client: ActorID,
    pub allocation_id: u64,
    #[serde(with = "crate::lotus_json")]
    pub data: Cid,
    pub size: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ClaimAllocationsParamsV11LotusJson {
    pub sectors: Vec<SectorAllocationClaimV11LotusJson>,
    pub all_or_nothing: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SectorAllocationClaimV11LotusJson {
    #[serde(with = "crate::lotus_json")]
    pub client: ActorID,
    pub allocation_id: u64,
    #[serde(with = "crate::lotus_json")]
    pub data: Cid,
    pub size: u64,
    #[serde(with = "crate::lotus_json")]
    pub sector: SectorNumber,
    #[serde(with = "crate::lotus_json")]
    pub sector_expiry: ChainEpoch,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct GetClaimsParamsLotusJson {
    #[schemars(with = "LotusJson<ActorID>")]
    #[serde(with = "crate::lotus_json")]
    pub provider: ActorID,
    #[schemars(with = "LotusJson<Vec<u64>>")]
    #[serde(with = "crate::lotus_json")]
    pub claim_ids: Vec<u64>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ExtendClaimTermsParamsLotusJson {
    pub terms: Vec<ClaimTermLotusJson>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ClaimTermLotusJson {
    #[schemars(with = "LotusJson<ActorID>")]
    #[serde(with = "crate::lotus_json")]
    pub provider: ActorID,
    pub claim_id: u64,
    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub term_max: ChainEpoch,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct RemoveExpiredClaimsParamsLotusJson {
    #[schemars(with = "LotusJson<ActorID>")]
    #[serde(with = "crate::lotus_json")]
    pub provider: ActorID,
    #[schemars(with = "LotusJson<Vec<u64>>")]
    #[serde(with = "crate::lotus_json")]
    pub claim_ids: Vec<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AllocationRequestsLotusJson {
    pub allocations: Vec<AllocationRequestLotusJson>,
    pub extensions: Vec<ClaimExtensionRequestLotusJson>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct AllocationRequestLotusJson {
    #[serde(with = "crate::lotus_json")]
    pub provider: ActorID,
    #[serde(with = "crate::lotus_json")]
    pub data: Cid,
    pub size: PaddedPieceSize,
    #[serde(with = "crate::lotus_json")]
    pub term_min: ChainEpoch,
    #[serde(with = "crate::lotus_json")]
    pub term_max: ChainEpoch,
    #[serde(with = "crate::lotus_json")]
    pub expiration: ChainEpoch,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ClaimExtensionRequestLotusJson {
    #[schemars(with = "LotusJson<ActorID>")]
    #[serde(with = "crate::lotus_json")]
    pub provider: ActorID,
    pub claim: u64,
    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub term_max: ChainEpoch,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UniversalReceiverParamsLotusJson {
    #[serde(rename = "Type_")]
    pub type_: u32,
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub payload: RawBytes,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct BytesParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub address: Address,
    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub deal_size: BigInt,
}

macro_rules! impl_constructor_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_verifreg_constructor_params_ $version>] {
                    use super::*;
                    type T = fil_actor_verifreg_state::[<v $version>]::ConstructorParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = ConstructorParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!("f01234"),
                                Self {
                                    root_key: Address::new_id(1234).into(),
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ConstructorParamsLotusJson(self.root_key.into())
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                root_key: lotus_json.0.into(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_verifier_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_verifreg_verifier_params_ $version>] {
                    use super::*;
                    type T = fil_actor_verifreg_state::[<v $version>]::VerifierParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = VerifierParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Address": "f01234",
                                    "Allowance": "1000000000000000000",
                                }),
                                Self {
                                    address: Address::new_id(1234).into(),
                                    allowance: BigInt::from(1000000000000000000u64),
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            VerifierParamsLotusJson {
                                address: self.address.into(),
                                allowance: self.allowance,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                address: lotus_json.address.into(),
                                allowance: lotus_json.allowance,
                            }
                        }
                    }
                }
            }
        )+
    };
}

// Implementation for RemoveVerifierParams
macro_rules! impl_remove_verifier_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_verifreg_remove_verifier_params_ $version>] {
                    use super::*;
                    type T = fil_actor_verifreg_state::[<v $version>]::RemoveVerifierParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = RemoveVerifierParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!("f01234"),
                                Self {
                                    verifier: Address::new_id(1234).into(),
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            RemoveVerifierParamsLotusJson(self.verifier.into())
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                verifier: lotus_json.0.into(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

// Implementation for RemoveExpiredAllocationsParams
macro_rules! impl_remove_expired_allocations_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_verifreg_remove_expired_allocations_params_ $version>] {
                    use super::*;
                    type T = fil_actor_verifreg_state::[<v $version>]::RemoveExpiredAllocationsParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = RemoveExpiredAllocationsParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Client": 1001,
                                    "AllocationIds": [1, 2, 3],
                                }),
                                Self {
                                    client: 1001,
                                    allocation_ids: vec![1, 2, 3],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            RemoveExpiredAllocationsParamsLotusJson {
                                client: self.client,
                                allocation_ids: self.allocation_ids,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                client: lotus_json.client,
                                allocation_ids: lotus_json.allocation_ids,
                            }
                        }
                    }
                }
            }
        )+
    };
}

// Implementation for GetClaimsParams
macro_rules! impl_get_claims_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_verifreg_get_claims_params_ $version>] {
                    use super::*;
                    type T = fil_actor_verifreg_state::[<v $version>]::GetClaimsParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = GetClaimsParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Provider": 1001,
                                    "ClaimIds": [1, 2, 3],
                                }),
                                Self {
                                    provider: 1001,
                                    claim_ids: vec![1, 2, 3],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            GetClaimsParamsLotusJson {
                                provider: self.provider,
                                claim_ids: self.claim_ids,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                provider: lotus_json.provider,
                                claim_ids: lotus_json.claim_ids,
                            }
                        }
                    }
                }
            }
        )+
    };
}

// Implementation for RemoveExpiredClaimsParams
macro_rules! impl_remove_expired_claims_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_verifreg_remove_expired_claims_params_ $version>] {
                    use super::*;
                    type T = fil_actor_verifreg_state::[<v $version>]::RemoveExpiredClaimsParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = RemoveExpiredClaimsParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Provider": 1001,
                                    "ClaimIds": [1, 2, 3],
                                }),
                                Self {
                                    provider: 1001,
                                    claim_ids: vec![1, 2, 3],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            RemoveExpiredClaimsParamsLotusJson {
                                provider: self.provider,
                                claim_ids: self.claim_ids,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                provider: lotus_json.provider,
                                claim_ids: lotus_json.claim_ids,
                            }
                        }
                    }
                }
            }
        )+
    };
}

// Implementation for UniversalReceiverParams (not version-specific)
impl HasLotusJson for fvm_actor_utils::receiver::UniversalReceiverParams {
    type LotusJson = UniversalReceiverParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Type": 1,
                "Payload": "dGVzdCBwYXlsb2Fk",
            }),
            Self {
                type_: 1,
                payload: RawBytes::new(b"test payload".to_vec()),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        UniversalReceiverParamsLotusJson {
            type_: self.type_,
            payload: self.payload,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            type_: lotus_json.type_,
            payload: lotus_json.payload,
        }
    }
}

// Implementation for ExtendClaimTermsParams
macro_rules! impl_extend_claim_terms_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_verifreg_extend_claim_terms_params_ $version>] {
                    use super::*;
                    type T = fil_actor_verifreg_state::[<v $version>]::ExtendClaimTermsParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = ExtendClaimTermsParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Terms": [
                                        {
                                            "Provider": 1001,
                                            "ClaimId": 1,
                                            "TermMax": 12345,
                                        }
                                    ],
                                }),
                                Self {
                                    terms: vec![fil_actor_verifreg_state::[<v $version>]::ClaimTerm {
                                        provider: 1001,
                                        claim_id: 1,
                                        term_max: 12345,
                                    }],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ExtendClaimTermsParamsLotusJson {
                                terms: self
                                    .terms
                                    .into_iter()
                                    .map(|term| ClaimTermLotusJson {
                                        provider: term.provider,
                                        claim_id: term.claim_id,
                                        term_max: term.term_max,
                                    })
                                    .collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                terms: lotus_json
                                    .terms
                                    .into_iter()
                                    .map(|term| fil_actor_verifreg_state::[<v $version>]::ClaimTerm {
                                        provider: term.provider,
                                        claim_id: term.claim_id,
                                        term_max: term.term_max,
                                    })
                                    .collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

// Implementation for RemoveDataCapParams with version-specific types
macro_rules! impl_remove_data_cap_params_v2 {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_verifreg_remove_data_cap_params_v2_ $version>] {
                    use super::*;
                    type T = fil_actor_verifreg_state::[<v $version>]::RemoveDataCapParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = RemoveDataCapParamsV2LotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "VerifiedClientToRemove": "f01234",
                                    "DataCapAmountToRemove": "1000000000000000000",
                                    "VerifierRequest1": {
                                        "Verifier": "f01235",
                                        "VerifierSignature": {
                                            "Type": 1,
                                            "Data": "dGVzdA==",
                                        }
                                    },
                                    "VerifierRequest2": {
                                        "Verifier": "f01236",
                                        "VerifierSignature": {
                                            "Type": 1,
                                            "Data": "dGVzdA==",
                                        }
                                    },
                                }),
                                Self {
                                    verified_client_to_remove: Address::new_id(1234).into(),
                                    data_cap_amount_to_remove: BigInt::from(1000000000000000000u64),
                                    verifier_request_1: fil_actor_verifreg_state::[<v $version>]::RemoveDataCapRequest {
                                        verifier: Address::new_id(1235).into(),
                                        signature: fvm_shared2::crypto::signature::Signature {
                                            sig_type: fvm_shared2::crypto::signature::SignatureType::Secp256k1,
                                            bytes: b"test".to_vec(),
                                        },
                                    },
                                    verifier_request_2: fil_actor_verifreg_state::[<v $version>]::RemoveDataCapRequest {
                                        verifier: Address::new_id(1236).into(),
                                        signature: fvm_shared2::crypto::signature::Signature {
                                            sig_type: fvm_shared2::crypto::signature::SignatureType::Secp256k1,
                                            bytes: b"test".to_vec(),
                                        },
                                    },
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            RemoveDataCapParamsV2LotusJson {
                                verified_client_to_remove: self.verified_client_to_remove.into(),
                                data_cap_amount_to_remove: self.data_cap_amount_to_remove,
                                verifier_request_1: RemoveDataCapRequestV2LotusJson {
                                    verifier: self.verifier_request_1.verifier.into(),
                                    signature: self.verifier_request_1.signature,
                                },
                                verifier_request_2: RemoveDataCapRequestV2LotusJson {
                                    verifier: self.verifier_request_2.verifier.into(),
                                    signature: self.verifier_request_2.signature,
                                },
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                verified_client_to_remove: lotus_json.verified_client_to_remove.into(),
                                data_cap_amount_to_remove: lotus_json.data_cap_amount_to_remove,
                                verifier_request_1: fil_actor_verifreg_state::[<v $version>]::RemoveDataCapRequest {
                                    verifier: lotus_json.verifier_request_1.verifier.into(),
                                    signature: lotus_json.verifier_request_1.signature,
                                },
                                verifier_request_2: fil_actor_verifreg_state::[<v $version>]::RemoveDataCapRequest {
                                    verifier: lotus_json.verifier_request_2.verifier.into(),
                                    signature: lotus_json.verifier_request_2.signature,
                                },
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_remove_data_cap_params_v3 {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_verifreg_remove_data_cap_params_v3_ $version>] {
                    use super::*;
                    type T = fil_actor_verifreg_state::[<v $version>]::RemoveDataCapParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = RemoveDataCapParamsV3LotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "VerifiedClientToRemove": "f01234",
                                    "DataCapAmountToRemove": "1000000000000000000",
                                    "VerifierRequest1": {
                                        "Verifier": "f01235",
                                        "VerifierSignature": {
                                            "Type": 1,
                                            "Data": "dGVzdA==",
                                        }
                                    },
                                    "VerifierRequest2": {
                                        "Verifier": "f01236",
                                        "VerifierSignature": {
                                            "Type": 1,
                                            "Data": "dGVzdA==",
                                        }
                                    },
                                }),
                                Self {
                                    verified_client_to_remove: Address::new_id(1234).into(),
                                    data_cap_amount_to_remove: BigInt::from(1000000000000000000u64),
                                    verifier_request_1: fil_actor_verifreg_state::[<v $version>]::RemoveDataCapRequest {
                                        verifier: Address::new_id(1235).into(),
                                        signature: fvm_shared3::crypto::signature::Signature {
                                            sig_type: fvm_shared3::crypto::signature::SignatureType::Secp256k1,
                                            bytes: b"test".to_vec(),
                                        },
                                    },
                                    verifier_request_2: fil_actor_verifreg_state::[<v $version>]::RemoveDataCapRequest {
                                        verifier: Address::new_id(1236).into(),
                                        signature: fvm_shared3::crypto::signature::Signature {
                                            sig_type: fvm_shared3::crypto::signature::SignatureType::Secp256k1,
                                            bytes: b"test".to_vec(),
                                        },
                                    },
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            RemoveDataCapParamsV3LotusJson {
                                verified_client_to_remove: self.verified_client_to_remove.into(),
                                data_cap_amount_to_remove: self.data_cap_amount_to_remove,
                                verifier_request_1: RemoveDataCapRequestV3LotusJson {
                                    verifier: self.verifier_request_1.verifier.into(),
                                    signature: self.verifier_request_1.signature,
                                },
                                verifier_request_2: RemoveDataCapRequestV3LotusJson {
                                    verifier: self.verifier_request_2.verifier.into(),
                                    signature: self.verifier_request_2.signature,
                                },
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                verified_client_to_remove: lotus_json.verified_client_to_remove.into(),
                                data_cap_amount_to_remove: lotus_json.data_cap_amount_to_remove,
                                verifier_request_1: fil_actor_verifreg_state::[<v $version>]::RemoveDataCapRequest {
                                    verifier: lotus_json.verifier_request_1.verifier.into(),
                                    signature: lotus_json.verifier_request_1.signature,
                                },
                                verifier_request_2: fil_actor_verifreg_state::[<v $version>]::RemoveDataCapRequest {
                                    verifier: lotus_json.verifier_request_2.verifier.into(),
                                    signature: lotus_json.verifier_request_2.signature,
                                },
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_remove_data_cap_params_v4 {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_verifreg_remove_data_cap_params_v4_ $version>] {
                    use super::*;
                    type T = fil_actor_verifreg_state::[<v $version>]::RemoveDataCapParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = RemoveDataCapParamsV4LotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "VerifiedClientToRemove": "f01234",
                                    "DataCapAmountToRemove": "1000000000000000000",
                                    "VerifierRequest1": {
                                        "Verifier": "f01235",
                                        "VerifierSignature": {
                                            "Type": 1,
                                            "Data": "dGVzdA==",
                                        }
                                    },
                                    "VerifierRequest2": {
                                        "Verifier": "f01236",
                                        "VerifierSignature": {
                                            "Type": 1,
                                            "Data": "dGVzdA==",
                                        }
                                    },
                                }),
                                Self {
                                    verified_client_to_remove: Address::new_id(1234).into(),
                                    data_cap_amount_to_remove: BigInt::from(1000000000000000000u64),
                                    verifier_request_1: fil_actor_verifreg_state::[<v $version>]::RemoveDataCapRequest {
                                        verifier: Address::new_id(1235).into(),
                                        signature: fvm_shared4::crypto::signature::Signature {
                                            sig_type: fvm_shared4::crypto::signature::SignatureType::Secp256k1,
                                            bytes: b"test".to_vec(),
                                        },
                                    },
                                    verifier_request_2: fil_actor_verifreg_state::[<v $version>]::RemoveDataCapRequest {
                                        verifier: Address::new_id(1236).into(),
                                        signature: fvm_shared4::crypto::signature::Signature {
                                            sig_type: fvm_shared4::crypto::signature::SignatureType::Secp256k1,
                                            bytes: b"test".to_vec(),
                                        },
                                    },
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            RemoveDataCapParamsV4LotusJson {
                                verified_client_to_remove: self.verified_client_to_remove.into(),
                                data_cap_amount_to_remove: self.data_cap_amount_to_remove,
                                verifier_request_1: RemoveDataCapRequestV4LotusJson {
                                    verifier: self.verifier_request_1.verifier.into(),
                                    signature: self.verifier_request_1.signature,
                                },
                                verifier_request_2: RemoveDataCapRequestV4LotusJson {
                                    verifier: self.verifier_request_2.verifier.into(),
                                    signature: self.verifier_request_2.signature,
                                },
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                verified_client_to_remove: lotus_json.verified_client_to_remove.into(),
                                data_cap_amount_to_remove: lotus_json.data_cap_amount_to_remove,
                                verifier_request_1: fil_actor_verifreg_state::[<v $version>]::RemoveDataCapRequest {
                                    verifier: lotus_json.verifier_request_1.verifier.into(),
                                    signature: lotus_json.verifier_request_1.signature,
                                },
                                verifier_request_2: fil_actor_verifreg_state::[<v $version>]::RemoveDataCapRequest {
                                    verifier: lotus_json.verifier_request_2.verifier.into(),
                                    signature: lotus_json.verifier_request_2.signature,
                                },
                            }
                        }
                    }
                }
            }
        )+
    };
}

// Implementation for ClaimAllocationsParams (v12-v16 with nested structure)
macro_rules! impl_claim_allocations_params_v12_plus {
    ($type_suffix:path: $($version:literal),+) => {
        $(
            paste! {
                mod [<impl_verifreg_claim_allocations_params_v12_plus_ $version>] {
                    use super::*;
                    type T = fil_actor_verifreg_state::[<v $version>]::ClaimAllocationsParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = ClaimAllocationsParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Sectors": [
                                        {
                                            "Sector": 1,
                                            "SectorExpiry": 12345,
                                            "Claims": [
                                                {
                                                    "Client": 1001,
                                                    "AllocationId": 1,
                                                    "Data": {"/": "bafk2bzacedbdmwqy4jrh4tgm7l77vz5fxb27jgmb2xkuprzzudbe2xj5u2nzm"},
                                                    "Size": 2048,
                                                }
                                            ],
                                        }
                                    ],
                                    "AllOrNothing": true,
                                }),
                                Self {
                                    sectors: vec![fil_actor_verifreg_state::[<v $version>]::SectorAllocationClaims {
                                        sector: 1,
                                        expiry: 12345,
                                        claims: vec![fil_actor_verifreg_state::[<v $version>]::AllocationClaim {
                                            client: 1001,
                                            allocation_id: 1,
                                            data: Cid::try_from(
                                                "bafk2bzacedbdmwqy4jrh4tgm7l77vz5fxb27jgmb2xkuprzzudbe2xj5u2nzm",
                                            )
                                            .unwrap(),
                                            size: $type_suffix::piece::PaddedPieceSize(2048),
                                        }],
                                    }],
                                    all_or_nothing: true,
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ClaimAllocationsParamsLotusJson {
                                sectors: self
                                    .sectors
                                    .into_iter()
                                    .map(|sector| SectorAllocationClaimsLotusJson {
                                        sector: sector.sector,
                                        expiry: sector.expiry,
                                        claims: sector
                                            .claims
                                            .into_iter()
                                            .map(|claim| AllocationClaimLotusJson {
                                                client: claim.client,
                                                allocation_id: claim.allocation_id,
                                                data: claim.data,
                                                size: claim.size.0,
                                            })
                                            .collect(),
                                    })
                                    .collect(),
                                all_or_nothing: self.all_or_nothing,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                sectors: lotus_json
                                    .sectors
                                    .into_iter()
                                    .map(
                                        |sector| fil_actor_verifreg_state::[<v $version>]::SectorAllocationClaims {
                                            sector: sector.sector,
                                            expiry: sector.expiry,
                                            claims: sector
                                                .claims
                                                .into_iter()
                                                .map(|claim| fil_actor_verifreg_state::[<v $version>]::AllocationClaim {
                                                    client: claim.client,
                                                    allocation_id: claim.allocation_id,
                                                    data: claim.data,
                                                    size: $type_suffix::piece::PaddedPieceSize(claim.size),
                                                })
                                                .collect(),
                                        },
                                    )
                                    .collect(),
                                all_or_nothing: lotus_json.all_or_nothing,
                            }
                        }
                    }
                }
            }
        )+
    };
}

// Implementation for ClaimAllocationsParams (v9-v11 with flat structure)
macro_rules! impl_claim_allocations_params_v11 {
    ($type_suffix:path: $($version:literal),+) => {
        $(
            paste! {
                mod [<impl_verifreg_claim_allocations_params_v11_ $version>] {
                    use super::*;
                    type T = fil_actor_verifreg_state::[<v $version>]::ClaimAllocationsParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = ClaimAllocationsParamsV11LotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Sectors": [
                                        {
                                            "Client": 1001,
                                            "AllocationId": 1,
                                            "Data": {"/": "bafk2bzacedbdmwqy4jrh4tgm7l77vz5fxb27jgmb2xkuprzzudbe2xj5u2nzm"},
                                            "Size": 2048,
                                            "Sector": 1,
                                            "SectorExpiry": 12345,
                                        }
                                    ],
                                    "AllOrNothing": true,
                                }),
                                Self {
                                    sectors: vec![fil_actor_verifreg_state::[<v $version>]::SectorAllocationClaim {
                                        client: 1001,
                                        allocation_id: 1,
                                        data: Cid::try_from(
                                            "bafk2bzacedbdmwqy4jrh4tgm7l77vz5fxb27jgmb2xkuprzzudbe2xj5u2nzm",
                                        )
                                        .unwrap(),
                                        size: $type_suffix::piece::PaddedPieceSize(2048),
                                        sector: 1,
                                        sector_expiry: 12345,
                                    }],
                                    all_or_nothing: true,
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ClaimAllocationsParamsV11LotusJson {
                                sectors: self
                                    .sectors
                                    .into_iter()
                                    .map(|sector| SectorAllocationClaimV11LotusJson {
                                        client: sector.client,
                                        allocation_id: sector.allocation_id,
                                        data: sector.data,
                                        size: sector.size.0,
                                        sector: sector.sector,
                                        sector_expiry: sector.sector_expiry,
                                    })
                                    .collect(),
                                all_or_nothing: self.all_or_nothing,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                sectors: lotus_json
                                    .sectors
                                    .into_iter()
                                    .map(|sector| fil_actor_verifreg_state::[<v $version>]::SectorAllocationClaim {
                                        client: sector.client,
                                        allocation_id: sector.allocation_id,
                                        data: sector.data,
                                        size: $type_suffix::piece::PaddedPieceSize(sector.size),
                                        sector: sector.sector,
                                        sector_expiry: sector.sector_expiry,
                                    })
                                    .collect(),
                                all_or_nothing: lotus_json.all_or_nothing,
                            }
                        }
                    }
                }
            }
        )+
    };
}

// Implementation for AllocationRequests (unified for all versions)
macro_rules! impl_allocation_requests {
    ($type_suffix:path: $($version:literal),+) => {
        $(
            paste! {
                mod [<impl_verifreg_allocation_requests_ $version>] {
                    use super::*;
                    type T = fil_actor_verifreg_state::[<v $version>]::AllocationRequests;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = AllocationRequestsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![(
                                json!({
                                    "Allocations": [
                                        {
                                            "Provider": 1001,
                                            "Data": {"/": "bafk2bzacedbdmwqy4jrh4tgm7l77vz5fxb27jgmb2xkuprzzudbe2xj5u2nzm"},
                                            "Size": 2048,
                                            "TermMin": 1000,
                                            "TermMax": 2000,
                                            "Expiration": 12345,
                                        }
                                    ],
                                    "Extensions": [
                                        {
                                            "Provider": 1002,
                                            "Claim": 1,
                                            "TermMax": 3000,
                                        }
                                    ],
                                }),
                                Self {
                                    allocations: vec![fil_actor_verifreg_state::[<v $version>]::AllocationRequest {
                                        provider: 1001,
                                        data: Cid::try_from(
                                            "bafk2bzacedbdmwqy4jrh4tgm7l77vz5fxb27jgmb2xkuprzzudbe2xj5u2nzm",
                                        )
                                        .unwrap(),
                                        size: $type_suffix::piece::PaddedPieceSize(2048),
                                        term_min: 1000,
                                        term_max: 2000,
                                        expiration: 12345,
                                    }],
                                    extensions: vec![fil_actor_verifreg_state::[<v $version>]::ClaimExtensionRequest {
                                        provider: 1002,
                                        claim: 1,
                                        term_max: 3000,
                                    }],
                                },
                            )]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            AllocationRequestsLotusJson {
                                allocations: self
                                    .allocations
                                    .into_iter()
                                    .map(|alloc| AllocationRequestLotusJson {
                                        provider: alloc.provider,
                                        data: alloc.data,
                                        size: PaddedPieceSize(alloc.size.0),
                                        term_min: alloc.term_min,
                                        term_max: alloc.term_max,
                                        expiration: alloc.expiration,
                                    })
                                    .collect(),
                                extensions: self
                                    .extensions
                                    .into_iter()
                                    .map(|ext| ClaimExtensionRequestLotusJson {
                                        provider: ext.provider,
                                        claim: ext.claim,
                                        term_max: ext.term_max,
                                    })
                                    .collect(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                allocations: lotus_json
                                    .allocations
                                    .into_iter()
                                    .map(|alloc| fil_actor_verifreg_state::[<v $version>]::AllocationRequest {
                                        provider: alloc.provider,
                                        data: alloc.data,
                                        size: $type_suffix::piece::PaddedPieceSize(alloc.size.0),
                                        term_min: alloc.term_min,
                                        term_max: alloc.term_max,
                                        expiration: alloc.expiration,
                                    })
                                    .collect(),
                                extensions: lotus_json
                                    .extensions
                                    .into_iter()
                                    .map(|ext| fil_actor_verifreg_state::[<v $version>]::ClaimExtensionRequest {
                                        provider: ext.provider,
                                        claim: ext.claim,
                                        term_max: ext.term_max,
                                    })
                                    .collect(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

// v8 has unique BytesParams for UseBytes/RestoreBytes methods
mod impl_verifreg_bytes_params_v8 {
    use super::*;
    type T = fil_actor_verifreg_state::v8::BytesParams;
    #[test]
    fn snapshots() {
        crate::lotus_json::assert_all_snapshots::<T>();
    }

    impl HasLotusJson for T {
        type LotusJson = BytesParamsLotusJson;

        #[cfg(test)]
        fn snapshots() -> Vec<(serde_json::Value, Self)> {
            vec![(
                json!({
                    "Address": "f01234",
                    "DealSize": "1048576", // 1MB
                }),
                Self {
                    address: Address::new_id(1234).into(),
                    deal_size: BigInt::from(1048576u64),
                },
            )]
        }

        fn into_lotus_json(self) -> Self::LotusJson {
            BytesParamsLotusJson {
                address: self.address.into(),
                deal_size: self.deal_size,
            }
        }

        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
            Self {
                address: lotus_json.address.into(),
                deal_size: lotus_json.deal_size,
            }
        }
    }
}

mod impl_verifreg_verifier_params_v8 {
    use super::*;
    type T = fil_actor_verifreg_state::v8::VerifierParams;
    #[test]
    fn snapshots() {
        crate::lotus_json::assert_all_snapshots::<T>();
    }

    impl HasLotusJson for T {
        type LotusJson = VerifierParamsLotusJson;

        #[cfg(test)]
        fn snapshots() -> Vec<(serde_json::Value, Self)> {
            vec![(
                json!({
                    "Address": "f01234",
                    "Allowance": "1000000000000000000",
                }),
                Self {
                    address: Address::new_id(1234).into(),
                    allowance: BigInt::from(1000000000000000000u64),
                },
            )]
        }

        fn into_lotus_json(self) -> Self::LotusJson {
            VerifierParamsLotusJson {
                address: self.address.into(),
                allowance: self.allowance,
            }
        }

        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
            Self {
                address: lotus_json.address.into(),
                allowance: lotus_json.allowance,
            }
        }
    }
}

mod impl_verifreg_add_verifier_client_params_v9 {
    use super::*;
    type T = fil_actor_verifreg_state::v9::AddVerifierClientParams;
    #[test]
    fn snapshots() {
        crate::lotus_json::assert_all_snapshots::<T>();
    }

    impl HasLotusJson for T {
        type LotusJson = VerifierParamsLotusJson;

        #[cfg(test)]
        fn snapshots() -> Vec<(serde_json::Value, Self)> {
            vec![(
                json!({
                    "Address": "f01234",
                    "Allowance": "1000000000000000000",
                }),
                Self {
                    address: Address::new_id(1234).into(),
                    allowance: BigInt::from(1000000000000000000u64),
                },
            )]
        }

        fn into_lotus_json(self) -> Self::LotusJson {
            VerifierParamsLotusJson {
                address: self.address.into(),
                allowance: self.allowance,
            }
        }

        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
            Self {
                address: lotus_json.address.into(),
                allowance: lotus_json.allowance,
            }
        }
    }
}

impl_constructor_params!(11, 12, 13, 14, 15, 16, 17);
impl_verifier_params!(10, 11, 12, 13, 14, 15, 16, 17); // Exclude v8,v9 due to different param names
impl_remove_verifier_params!(11, 12, 13, 14, 15, 16, 17);
impl_remove_expired_allocations_params!(9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_get_claims_params!(9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_remove_expired_claims_params!(9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_extend_claim_terms_params!(9, 10, 11, 12, 13, 14, 15, 16, 17);

impl_remove_data_cap_params_v2!(8, 9);
impl_remove_data_cap_params_v3!(10, 11);
impl_remove_data_cap_params_v4!(12, 13, 14, 15, 16, 17);

impl_claim_allocations_params_v11!(fvm_shared2: 9);
impl_claim_allocations_params_v11!(fvm_shared3: 10, 11);
impl_claim_allocations_params_v12_plus!(fvm_shared4: 12, 13, 14, 15, 16, 17);
impl_allocation_requests!(fvm_shared4: 12, 13, 14, 15, 16, 17);
impl_allocation_requests!(fvm_shared3: 11);
