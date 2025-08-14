// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use fil_actor_market_state::v16::Label;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum LabelLotusJson {
    String(String),
    Bytes(Vec<u8>),
}

impl HasLotusJson for Label {
    type LotusJson = LabelLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        match self {
            Label::Bytes(bytes) => LabelLotusJson::Bytes(bytes),
            Label::String(string) => LabelLotusJson::String(string),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        match lotus_json {
            LabelLotusJson::Bytes(bytes) => Label::Bytes(bytes),
            LabelLotusJson::String(string) => Label::String(string),
        }
    }
}
