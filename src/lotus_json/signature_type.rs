// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::crypto::SignatureType;

impl HasLotusJson for SignatureType {
    // TODO: Lotus also accepts ints when deserializing this from JSON
    //       https://github.com/filecoin-project/lotus/blob/v1.23.3/chain/types/keystore.go#L47
    type LotusJson = Stringify<Self>;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("bls"), SignatureType::Bls)]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.into()
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        lotus_json.into_inner()
    }
}
