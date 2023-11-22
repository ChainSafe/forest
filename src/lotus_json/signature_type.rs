// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::crypto::SignatureType;

// Lotus uses signature types under two names: `KeyType` and `SigType`.
// `KeyType` can be deserialized from a string but `SigType` must always be an
// integer. For more information, see
// https://github.com/filecoin-project/go-state-types/blob/a0445436230e221ab1828ad170623fcfe00c8263/crypto/signature.go
// and
// https://github.com/filecoin-project/lotus/blob/7bb1f98ac6f5a6da2cc79afc26d8cd9fe323eb30/chain/types/keystore.go#L47

#[derive(Deserialize, Serialize)]
#[serde(untagged)] // try an int, then a string
pub enum SignatureTypeLotusJson {
    Integer(SignatureType),
    String(Stringify<SignatureType>),
}

impl HasLotusJson for SignatureType {
    type LotusJson = SignatureTypeLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("bls"), SignatureType::Bls)]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        SignatureTypeLotusJson::Integer(self)
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        match lotus_json {
            SignatureTypeLotusJson::Integer(inner)
            | SignatureTypeLotusJson::String(Stringify(inner)) => inner,
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
