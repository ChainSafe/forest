// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use ahash::HashMap as AHashMap;
use std::hash::Hash;

impl<K, V> HasLotusJson for AHashMap<K, V>
where
    K: Serialize + DeserializeOwned + Eq + Hash,
    V: HasLotusJson,
{
    type LotusJson = AHashMap<K, <V as HasLotusJson>::LotusJson>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        unimplemented!()
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.into_iter()
            .map(|(k, v)| (k, v.into_lotus_json()))
            .collect()
    }

    fn from_lotus_json(value: Self::LotusJson) -> Self {
        value
            .into_iter()
            .map(|(k, v)| (k, V::from_lotus_json(v)))
            .collect()
    }
}
