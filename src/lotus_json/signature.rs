// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::crypto::{Signature, SignatureType};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SignatureLotusJson {
    r#type: LotusJson<SignatureType>,
    data: LotusJson<Vec<u8>>,
}

impl HasLotusJson for Signature {
    type LotusJson = SignatureLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({"Type": "bls", "Data": "aGVsbG8gd29ybGQh"}),
            Signature {
                sig_type: crate::shim::crypto::SignatureType::Bls,
                bytes: Vec::from_iter(*b"hello world!"),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let Self { sig_type, bytes } = self;
        Self::LotusJson {
            r#type: sig_type.into(),
            data: bytes.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson { r#type, data } = lotus_json;
        Self {
            sig_type: r#type.into_inner(),
            bytes: data.into_inner(),
        }
    }
}
