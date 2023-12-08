// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::eth::{Address, BlockNumberOrHash, Predefined};

impl HasLotusJson for Address {
    type LotusJson = String;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("0x0"), Address::default())]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        format!("{:#x}", self.0)
    }

    fn from_lotus_json(address: Self::LotusJson) -> Self {
        Address::from_str(&address).unwrap()
    }
}

impl HasLotusJson for BlockNumberOrHash {
    type LotusJson = String;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        unimplemented!()
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        match self {
            Self::PredefinedBlock(predefined) => predefined.to_string(),
            Self::BlockNumber(number) => format!("0x{:x}", number),
            Self::BlockHash(hash, _require_canonical) => format!("0x{:x}", hash.0),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        match lotus_json.as_str() {
            "earliest" => return Self::PredefinedBlock(Predefined::Earliest),
            "pending" => return Self::PredefinedBlock(Predefined::Pending),
            "latest" => return Self::PredefinedBlock(Predefined::Latest),
            _ => (),
        };

        if lotus_json.len() > 2 && &lotus_json[..2] == "0x" {
            if let Ok(number) = u64::from_str_radix(&lotus_json[2..], 16) {
                return Self::BlockNumber(number);
            }
        }

        Self::PredefinedBlock(Predefined::Latest)
    }
}
