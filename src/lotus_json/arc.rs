// Copyright 2019-2026 ChainSafe Systems
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

// `Arc<str>` can't reuse the blanket impl above (which requires `T: Sized`,
// and `str` is not). `Arc<str>` already satisfies every `HasLotusJson`
// consumer bound directly (`Serialize + DeserializeOwned + JsonSchema +
// Clone + 'static`), so we make `LotusJson = Self` and keep the conversions
// as identity — serializing the cached value takes no allocation.
impl HasLotusJson for Arc<str> {
    type LotusJson = Self;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        unimplemented!("tests are trivial for HasLotusJson<LotusJson = Self>")
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        lotus_json
    }
}
