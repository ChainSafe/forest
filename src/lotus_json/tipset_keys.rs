// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::TipsetKey;
use ::cid::Cid;
use ::nonempty::NonEmpty;

// must newtype so can impl JsonSchema
#[derive(Serialize, Deserialize)]
pub struct TipsetKeyLotusJson(LotusJson<NonEmpty<Cid>>);

impl JsonSchema for TipsetKeyLotusJson {
    fn schema_name() -> String {
        String::from("TipsetKeyLotusJson")
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> Schema {
        gen.subschema_for::<LotusJson<Vec<Cid>>>()
    }
}

impl HasLotusJson for TipsetKey {
    type LotusJson = TipsetKeyLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!([{"/": "baeaaaaa"}]),
            ::nonempty::nonempty![::cid::Cid::default()].into(),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        TipsetKeyLotusJson(LotusJson(self.into_cids()))
    }

    fn from_lotus_json(TipsetKeyLotusJson(lotus_json): Self::LotusJson) -> Self {
        Self::from(lotus_json.into_inner())
    }
}
