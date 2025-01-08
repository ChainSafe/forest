// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;

use crate::{key_management::KeyInfo, shim::crypto::SignatureType};

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "KeyInfo")]
pub struct KeyInfoLotusJson {
    #[schemars(with = "LotusJson<SignatureType>")]
    #[serde(with = "crate::lotus_json")]
    r#type: SignatureType,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    private_key: Vec<u8>,
}

impl HasLotusJson for KeyInfo {
    type LotusJson = KeyInfoLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Type": 2,
                "PrivateKey": "aGVsbG8gd29ybGQh"
            }),
            Self::new(
                crate::shim::crypto::SignatureType::Bls,
                b"hello world!".to_vec(),
            ),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let (key_type, private_key) = (self.key_type(), self.private_key());
        Self::LotusJson {
            r#type: (*key_type),
            private_key: private_key.clone(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            r#type,
            private_key,
        } = lotus_json;
        Self::new(r#type, private_key)
    }
}
