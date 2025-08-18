// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::crypto::{Signature, SignatureType};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "Signature")]
pub struct SignatureLotusJson {
    #[schemars(with = "LotusJson<SignatureType>")]
    #[serde(with = "crate::lotus_json")]
    r#type: SignatureType,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    data: Vec<u8>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "Signature")]
pub struct SignatureV2LotusJson {
    #[schemars(with = "LotusJson<fvm_shared2::crypto::signature::SignatureType>")]
    #[serde(with = "crate::lotus_json")]
    r#type: fvm_shared2::crypto::signature::SignatureType,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    data: Vec<u8>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "Signature")]
pub struct SignatureV3LotusJson {
    #[schemars(with = "LotusJson<fvm_shared3::crypto::signature::SignatureType>")]
    #[serde(with = "crate::lotus_json")]
    r#type: fvm_shared3::crypto::signature::SignatureType,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    data: Vec<u8>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "Signature")]
pub struct SignatureV4LotusJson {
    #[schemars(with = "LotusJson<fvm_shared4::crypto::signature::SignatureType>")]
    #[serde(with = "crate::lotus_json")]
    r#type: fvm_shared4::crypto::signature::SignatureType,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    data: Vec<u8>,
}

impl HasLotusJson for Signature {
    type LotusJson = SignatureLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({"Type": 2, "Data": "aGVsbG8gd29ybGQh"}),
            Signature {
                sig_type: crate::shim::crypto::SignatureType::Bls,
                bytes: Vec::from_iter(*b"hello world!"),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let Self { sig_type, bytes } = self;
        Self::LotusJson {
            r#type: sig_type,
            data: bytes,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson { r#type, data } = lotus_json;
        Self {
            sig_type: r#type,
            bytes: data,
        }
    }
}

impl HasLotusJson for fvm_shared2::crypto::signature::Signature {
    type LotusJson = SignatureV2LotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({"Type": 2, "Data": "aGVsbG8gd29ybGQh"}),
            Self {
                sig_type: fvm_shared2::crypto::signature::SignatureType::BLS,
                bytes: Vec::from_iter(*b"hello world!"),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        SignatureV2LotusJson {
            r#type: self.sig_type,
            data: self.bytes,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            sig_type: lotus_json.r#type,
            bytes: lotus_json.data,
        }
    }
}

impl HasLotusJson for fvm_shared3::crypto::signature::Signature {
    type LotusJson = SignatureV3LotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({"Type": 1, "Data": "aGVsbG8gd29ybGQh"}),
            Self {
                sig_type: fvm_shared3::crypto::signature::SignatureType::Secp256k1,
                bytes: Vec::from_iter(*b"hello world!"),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        SignatureV3LotusJson {
            r#type: self.sig_type,
            data: self.bytes,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            sig_type: lotus_json.r#type,
            bytes: lotus_json.data,
        }
    }
}

impl HasLotusJson for fvm_shared4::crypto::signature::Signature {
    type LotusJson = SignatureV4LotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({"Type": 1, "Data": "aGVsbG8gd29ybGQh"}),
            Self {
                sig_type: fvm_shared4::crypto::signature::SignatureType::Secp256k1,
                bytes: Vec::from_iter(*b"hello world!"),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        SignatureV4LotusJson {
            r#type: self.sig_type,
            data: self.bytes,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            sig_type: lotus_json.r#type,
            bytes: lotus_json.data,
        }
    }
}
