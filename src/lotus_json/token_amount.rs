// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::econ::TokenAmount;
use num::BigInt;

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)] // name the field for clarity
#[schemars(rename = "TokenAmount")]
pub struct TokenAmountLotusJson {
    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    attos: BigInt,
}

impl HasLotusJson for TokenAmount {
    type LotusJson = TokenAmountLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("1"), TokenAmount::from_atto(1))]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        Self::LotusJson {
            attos: self.atto().clone(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson { attos } = lotus_json;
        Self::from_atto(attos)
    }
}
