// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

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

impl HasLotusJson for ApiTipsetKey {
    type LotusJson = LotusJson<Vec<Cid>>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        LotusJson(
            self.0
                .map(|ts| ts.into_cids().into_iter().collect::<Vec<Cid>>())
                .unwrap_or_default(),
        )
    }

    fn from_lotus_json(LotusJson(lotus_json): Self::LotusJson) -> Self {
        Self(NonEmpty::from_vec(lotus_json).map(From::from))
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
