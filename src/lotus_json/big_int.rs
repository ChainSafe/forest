// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use num::BigInt;

#[derive(Serialize, Deserialize)]
pub struct BigIntLotusJson(#[serde(with = "stringify")] BigInt);

impl HasLotusJson for BigInt {
    type LotusJson = BigIntLotusJson;

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
