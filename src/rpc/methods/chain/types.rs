// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Eq, PartialEq, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ObjStat {
    pub size: usize,
    pub links: usize,
}
lotus_json_with_self!(ObjStat);
