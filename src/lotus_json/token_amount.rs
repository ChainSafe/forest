// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::econ::TokenAmount;
use num::BigInt;

#[derive(Serialize, Deserialize)]
#[serde(transparent)] // name the field for clarity
pub struct TokenAmountLotusJson {
    attos: LotusJson<BigInt>,
}

impl HasLotusJson for TokenAmount {
    type LotusJson = TokenAmountLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("1"), TokenAmount::from_atto(1))]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        Self::LotusJson {
            attos: self.atto().clone().into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson { attos } = lotus_json;
        Self::from_atto(attos.into_inner())
    }
}
