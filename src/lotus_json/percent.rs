// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::percent::Percent;

impl HasLotusJson for Percent {
    type LotusJson = f64;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!(1.25), Percent(125)), (json!(1.10), Percent(110))]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.0 as f64 / 100.0
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let scaled = lotus_json * 100.0;
        let rounded = scaled.round();
        if (scaled - rounded).abs() > 1e-6 {
            panic!("ratio may only have two decimals: {lotus_json}");
        }
        Percent(rounded as u64)
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for Percent {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let whole = u32::arbitrary(g) % 10_000;
        let frac = u32::arbitrary(g) % 100;
        Percent(u64::from(whole) * 100 + u64::from(frac))
    }
}
