// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;

use crate::{key_management::KeyInfo, shim::crypto::SignatureType};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct KeyInfoLotusJson {
    r#type: LotusJson<SignatureType>,
    private_key: LotusJson<Vec<u8>>,
}

impl HasLotusJson for KeyInfo {
    type LotusJson = KeyInfoLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Type": "bls",
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
            r#type: (*key_type).into(),
            private_key: private_key.clone().into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            r#type,
            private_key,
        } = lotus_json;
        Self::new(r#type.into_inner(), private_key.into_inner())
    }
}
