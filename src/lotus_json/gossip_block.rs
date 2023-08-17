// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use crate::blocks::{BlockHeader, GossipBlock};
use ::cid::Cid;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GossipBlockLotusJson {
    header: LotusJson<BlockHeader>,
    bls_messages: LotusJson<Vec<Cid>>,
    secpk_messages: LotusJson<Vec<Cid>>,
}

impl HasLotusJson for GossipBlock {
    type LotusJson = GossipBlockLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        use serde_json::json;

        vec![(
            json!({
                "Header": {
                    "BeaconEntries": null,
                    "Miner": "f00",
                    "Parents": null,
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
            header: header.into(),
            bls_messages: bls_messages.into(),
            secpk_messages: secpk_messages.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            header,
            bls_messages,
            secpk_messages,
        } = lotus_json;
        Self {
            header: header.into_inner(),
            bls_messages: bls_messages.into_inner(),
            secpk_messages: secpk_messages.into_inner(),
        }
    }
}
