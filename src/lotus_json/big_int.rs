// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use num::BigInt;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "BigInt")]
pub struct BigIntLotusJson(
    #[schemars(with = "String")]
    #[serde(with = "crate::lotus_json::stringify")]
    BigInt,
);

impl HasLotusJson for BigInt {
    type LotusJson = BigIntLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("1"), BigInt::from(1))]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        BigIntLotusJson(self)
    }

    fn from_lotus_json(BigIntLotusJson(big_int): Self::LotusJson) -> Self {
        big_int
    }
}
