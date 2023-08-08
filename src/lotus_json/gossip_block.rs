// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use crate::blocks::GossipBlock;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GossipBlockLotusJson {
    pub header: <crate::blocks::BlockHeader as HasLotusJson>::LotusJson,
    pub bls_messages: VecLotusJson<CidLotusJson>,
    pub secpk_messages: VecLotusJson<CidLotusJson>,
}

#[test]
fn snapshots() {
    assert_all_snapshots::<GossipBlock>()
}

#[cfg(test)]
quickcheck! {
    fn quickcheck(val: GossipBlock) -> () {
        assert_unchanged_via_json(val)
    }
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
}

impl From<GossipBlockLotusJson> for GossipBlock {
    fn from(value: GossipBlockLotusJson) -> Self {
        let GossipBlockLotusJson {
            header,
            bls_messages,
            secpk_messages,
        } = value;
        Self {
            header: header.into(),
            bls_messages: bls_messages.into(),
            secpk_messages: secpk_messages.into(),
        }
    }
}

impl From<GossipBlock> for GossipBlockLotusJson {
    fn from(value: GossipBlock) -> Self {
        let GossipBlock {
            header,
            bls_messages,
            secpk_messages,
        } = value;
        Self {
            header: header.into(),
            bls_messages: bls_messages.into(),
            secpk_messages: secpk_messages.into(),
        }
    }
}
