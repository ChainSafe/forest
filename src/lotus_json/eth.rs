// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::eth::{Address, BlockNumberOrHash, Hash, Predefined};

impl HasLotusJson for Address {
    type LotusJson = String;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("0x0"), Address::default())]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        format!("{:#x}", self.0)
    }

    fn from_lotus_json(address: Self::LotusJson) -> Self {
        Address(Hash::from_str(&address).unwrap())
    }
}

impl HasLotusJson for BlockNumberOrHash {
    type LotusJson = String;

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
        if let Some(value) = predefined_block {
            value.to_string()
        } else {
            unimplemented!()
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            predefined_block: Some(Predefined::Latest),
            block_number: None,
            block_hash: None,
            require_canonical: false,
        }
    }
}
