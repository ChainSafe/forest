// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::crypto::SignatureType;

// Lotus uses signature types under two names: `KeyType` and `SigType`.
// `KeyType` can be deserialized from a string but `SigType` must always be an
// integer. For more information, see
// https://github.com/filecoin-project/go-state-types/blob/a0445436230e221ab1828ad170623fcfe00c8263/crypto/signature.go
// and
// https://github.com/filecoin-project/lotus/blob/7bb1f98ac6f5a6da2cc79afc26d8cd9fe323eb30/chain/types/keystore.go#L47

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(untagged)] // try an int, then a string
pub enum SignatureTypeLotusJson {
    Integer(#[schemars(with = "u8")] SignatureType),
    String(
        #[serde(with = "crate::lotus_json::stringify")]
        #[schemars(with = "SignatureType")]
        SignatureType,
    ),
}

impl HasLotusJson for SignatureType {
    type LotusJson = SignatureTypeLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!(2), SignatureType::Bls)]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        SignatureTypeLotusJson::Integer(self)
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        match lotus_json {
            SignatureTypeLotusJson::Integer(inner) | SignatureTypeLotusJson::String(inner) => inner,
        }
    }
}

#[test]
fn deserialize_integer() {
    pretty_assertions::assert_eq!(
        SignatureType::Bls,
        serde_json::from_value(json!(2)).unwrap()
    );
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum SignatureTypeV2LotusJson {
    Integer(#[schemars(with = "u8")] fvm_shared2::crypto::signature::SignatureType),
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum SignatureTypeV3LotusJson {
    Integer(#[schemars(with = "u8")] fvm_shared3::crypto::signature::SignatureType),
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum SignatureTypeV4LotusJson {
    Integer(#[schemars(with = "u8")] fvm_shared4::crypto::signature::SignatureType),
}

impl HasLotusJson for fvm_shared2::crypto::signature::SignatureType {
    type LotusJson = SignatureTypeV2LotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![
            (
                json!(1),
                fvm_shared2::crypto::signature::SignatureType::Secp256k1,
            ),
            (json!(2), fvm_shared2::crypto::signature::SignatureType::BLS),
        ]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        SignatureTypeV2LotusJson::Integer(self)
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        match lotus_json {
            SignatureTypeV2LotusJson::Integer(inner) => inner,
        }
    }
}

impl HasLotusJson for fvm_shared3::crypto::signature::SignatureType {
    type LotusJson = SignatureTypeV3LotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![
            (
                json!(1),
                fvm_shared3::crypto::signature::SignatureType::Secp256k1,
            ),
            (json!(2), fvm_shared3::crypto::signature::SignatureType::BLS),
        ]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        SignatureTypeV3LotusJson::Integer(self)
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        match lotus_json {
            SignatureTypeV3LotusJson::Integer(inner) => inner,
        }
    }
}

impl HasLotusJson for fvm_shared4::crypto::signature::SignatureType {
    type LotusJson = SignatureTypeV4LotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![
            (
                json!(1),
                fvm_shared4::crypto::signature::SignatureType::Secp256k1,
            ),
            (json!(2), fvm_shared4::crypto::signature::SignatureType::BLS),
        ]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        SignatureTypeV4LotusJson::Integer(self)
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        match lotus_json {
            SignatureTypeV4LotusJson::Integer(inner) => inner,
        }
    }
}
