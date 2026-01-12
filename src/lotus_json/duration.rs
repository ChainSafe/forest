// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use std::time::Duration;

impl HasLotusJson for Duration {
    type LotusJson = u64;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!(15000000000_u64), Duration::from_secs(15))]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.as_nanos() as _
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self::from_nanos(lotus_json)
    }
}
