// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::eth::{Address, BlockNumberOrHash, Hash};

impl HasLotusJson for Address {
    type LotusJson = Stringify<Address>;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("0x0"), Address::default())]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.into()
    }

    fn from_lotus_json(Stringify(address): Self::LotusJson) -> Self {
        address
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BlockNumberOrHashLotusJson {
    predefined_block: LotusJson<String>,
    block_number: LotusJson<u64>,
    block_hash: LotusJson<Hash>,
    require_canonical: LotusJson<bool>,
}

impl HasLotusJson for BlockNumberOrHash {
    type LotusJson = BlockNumberOrHashLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        unimplemented!()
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let Self {
            predefined_block,
            block_number,
            block_hash,
            require_canonical,
        } = self;
        Self::LotusJson {
            predefined_block: predefined_block.into(),
            block_number: block_number.into(),
            block_hash: block_hash.into(),
            require_canonical: require_canonical.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            predefined_block,
            block_number,
            block_hash,
            require_canonical,
        } = lotus_json;
        Self {
            predefined_block: predefined_block.into_inner(),
            block_number: block_number.into_inner(),
            block_hash: block_hash.into_inner(),
            require_canonical: require_canonical.into_inner(),
        }
    }
}
