// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use std::sync::Arc;

impl<T: HasLotusJson + Clone> HasLotusJson for Arc<T> {
    type LotusJson = T::LotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        T::snapshots()
            .into_iter()
            .map(|(k, v)| (k, Arc::new(v)))
            .collect()
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        (*self).clone().into_lotus_json()
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Arc::new(T::from_lotus_json(lotus_json))
    }
}
