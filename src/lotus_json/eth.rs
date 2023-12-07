// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::eth::{Address, BigInt, BlockNumberOrHash, Predefined};
use num::traits::Num;

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

impl HasLotusJson for BigInt {
    type LotusJson = String;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("0x0"), BigInt::default())]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        format!("0x{:x}", self.0)
    }

    fn from_lotus_json(big_int: Self::LotusJson) -> Self {
        BigInt(num::BigInt::from_str_radix(&big_int.as_str()[2..], 16).unwrap())
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
            return value.to_string();
        }
        if let Some(number) = block_number {
            return format!("0x{:x}", number);
        }
        if let Some(block_hash) = block_hash {
            return format!("0x{:x}", block_hash.0);
        }
        unimplemented!()
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let predefined = match lotus_json.as_str() {
            "earliest" => Some(Predefined::Earliest),
            "pending" => Some(Predefined::Pending),
            "latest" => Some(Predefined::Latest),
            _ => None,
        };

        let number = if lotus_json.len() > 2 && &lotus_json[..2] == "0x" {
            if let Ok(number) = u64::from_str_radix(&lotus_json[2..], 16) {
                Some(number)
            } else {
                None
            }
        } else {
            None
        };
        Self {
            predefined_block: predefined,
            block_number: number,
            block_hash: None,
            require_canonical: false,
        }
    }
}
