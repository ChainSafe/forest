// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use num::BigInt;

impl HasLotusJson for BigInt {
    type LotusJson = Stringify<BigInt>;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("1"), BigInt::from(1))]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.into()
    }

    fn from_lotus_json(Stringify(big_int): Self::LotusJson) -> Self {
        big_int
    }
}
