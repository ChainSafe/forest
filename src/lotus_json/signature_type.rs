// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::crypto::SignatureType;

#[derive(Deserialize, Serialize)]
#[serde(untagged)] // try an int, then a string
pub enum SignatureTypeLotusJson {
    // Lotus also accepts ints when deserializing - we need this for our test vectors
    // https://github.com/filecoin-project/lotus/blob/v1.23.3/chain/types/keystore.go#L47
    Integer(SignatureType),
    String(Stringify<SignatureType>),
}

impl HasLotusJson for SignatureType {
    type LotusJson = SignatureTypeLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("bls"), SignatureType::Bls)]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        // always serialize as a string, since lotus deprecates ints
        SignatureTypeLotusJson::String(Stringify(self))
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
