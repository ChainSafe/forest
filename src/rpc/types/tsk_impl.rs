// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::lotus_json::HasLotusJson;
use itertools::Itertools as _;

impl From<TipsetKey> for ApiTipsetKey {
    fn from(value: TipsetKey) -> Self {
        Self(Some(value))
    }
}

impl From<&TipsetKey> for ApiTipsetKey {
    fn from(value: &TipsetKey) -> Self {
        value.clone().into()
    }
}

// TODO(forest): https://github.com/ChainSafe/forest/issues/4032
//               this shouldn't exist
impl HasLotusJson for ApiTipsetKey {
    type LotusJson = <Vec<Cid> as HasLotusJson>::LotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.0
            .map(|ts| ts.into_cids().into_iter().collect_vec())
            .unwrap_or_default()
            .into_lotus_json()
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self(
            NonEmpty::new(HasLotusJson::from_lotus_json(lotus_json))
                .map(Into::into)
                .ok(),
        )
    }
}

impl std::fmt::Display for ApiTipsetKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(tsk) = &self.0 {
            write!(f, "{tsk}")
        } else {
            write!(f, "")
        }
    }
}
