// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

// This code looks odd so we can
// - use #[serde(with = "...")]
// - de/ser empty vecs as null
#[derive(Serialize, Deserialize)]
pub struct VecU8LotusJson(Option<Inner>);

#[derive(Serialize, Deserialize)]
struct Inner(#[serde(with = "base64_standard")] Vec<u8>);

impl HasLotusJson for Vec<u8> {
    type LotusJson = VecU8LotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![
            (json!("aGVsbG8gd29ybGQh"), Vec::from_iter(*b"hello world!")),
            (json!(null), Vec::new()),
        ]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        match self.is_empty() {
            true => VecU8LotusJson(None),
            false => VecU8LotusJson(Some(Inner(self))),
        }
    }

    fn from_lotus_json(value: Self::LotusJson) -> Self {
        match value {
            VecU8LotusJson(Some(Inner(vec))) => vec,
            VecU8LotusJson(None) => Vec::new(),
        }
    }
}
