// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use crate::blocks::{CachingBlockHeader, GossipBlock};
use ::cid::Cid;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "GossipBlock")]
pub struct GossipBlockLotusJson {
    #[schemars(with = "LotusJson<CachingBlockHeader>")]
    #[serde(with = "crate::lotus_json")]
    header: CachingBlockHeader,
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    bls_messages: Vec<Cid>,
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    secpk_messages: Vec<Cid>,
}

impl HasLotusJson for GossipBlock {
    type LotusJson = GossipBlockLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        use serde_json::json;

        vec![(
            json!({
                "Header": {
                    "BeaconEntries": null,
                    "Miner": "f00",
                    "Parents": [{"/":"bafyreiaqpwbbyjo4a42saasj36kkrpv4tsherf2e7bvezkert2a7dhonoi"}],
                    "ParentWeight": "0",
                    "Height": 0,
                    "ParentStateRoot": {
                        "/": "baeaaaaa"
                    },
                    "ParentMessageReceipts": {
                        "/": "baeaaaaa"
                    },
                    "Messages": {
                        "/": "baeaaaaa"
                    },
                    "WinPoStProof": null,
                    "Timestamp": 0,
                    "ForkSignaling": 0,
                    "ParentBaseFee": "0",
                },
                "BlsMessages": null,
                "SecpkMessages": null
            }),
            GossipBlock::default(),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let Self {
            header,
            bls_messages,
            secpk_messages,
        } = self;
        Self::LotusJson {
            header,
            bls_messages,
            secpk_messages,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            header,
            bls_messages,
            secpk_messages,
        } = lotus_json;
        Self {
            header,
            bls_messages,
            secpk_messages,
        }
    }
}
