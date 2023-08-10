// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::crypto::Signature;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SignatureLotusJson {
    r#type: SignatureTypeLotusJson,
    data: VecU8LotusJson,
}

impl HasLotusJson for Signature {
    type LotusJson = SignatureLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({"Type": 2, "Data": "aGVsbG8gd29ybGQh"}),
            Signature {
                sig_type: crate::shim::crypto::SignatureType::Bls,
                bytes: Vec::from_iter(*b"hello world!"),
            },
        )]
    }
}

impl From<SignatureLotusJson> for Signature {
    fn from(value: SignatureLotusJson) -> Self {
        let SignatureLotusJson { r#type, data } = value;
        Self {
            sig_type: r#type.into(),
            bytes: data.into(),
        }
    }
}

impl From<Signature> for SignatureLotusJson {
    fn from(value: Signature) -> Self {
        let Signature { sig_type, bytes } = value;
        Self {
            r#type: sig_type.into(),
            data: bytes.into(),
        }
    }
}
