// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use crate::shim::econ::TokenAmount;

#[derive(Serialize, Deserialize)]
#[serde(transparent)] // name the field for clarity
pub struct TokenAmountLotusJson {
    attos: BigIntLotusJson,
}

impl HasLotusJson for TokenAmount {
    type LotusJson = TokenAmountLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("1"), TokenAmount::from_atto(1))]
    }
}

impl From<TokenAmount> for TokenAmountLotusJson {
    fn from(value: TokenAmount) -> Self {
        Self {
            attos: value.atto().clone().into(),
        }
    }
}

impl From<TokenAmountLotusJson> for TokenAmount {
    fn from(value: TokenAmountLotusJson) -> Self {
        Self::from_atto(value.attos)
    }
}
