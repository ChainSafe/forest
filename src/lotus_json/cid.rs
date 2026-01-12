// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "Cid")]
pub struct CidLotusJson {
    #[schemars(with = "String")]
    #[serde(rename = "/", with = "crate::lotus_json::stringify")]
    slash: ::cid::Cid,
}

impl HasLotusJson for ::cid::Cid {
    type LotusJson = CidLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!({"/": "baeaaaaa"}), ::cid::Cid::default())]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        Self::LotusJson { slash: self }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson { slash } = lotus_json;
        slash
    }
}
