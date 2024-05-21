// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "Cid")]
pub struct CidLotusJsonGeneric<const S: usize> {
    #[schemars(with = "String")]
    #[serde(rename = "/", with = "crate::lotus_json::stringify")]
    slash: ::cid::CidGeneric<S>,
}

impl<const S: usize> HasLotusJson for ::cid::CidGeneric<S> {
    type LotusJson = CidLotusJsonGeneric<S>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        unimplemented!("only Cid<64> is tested, below")
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        Self::LotusJson { slash: self }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson { slash } = lotus_json;
        slash
    }
}

#[test]
fn snapshots() {
    assert_one_snapshot(json!({"/": "baeaaaaa"}), ::cid::Cid::default());
}

#[cfg(test)]
quickcheck! {
    fn quickcheck(val: ::cid::Cid) -> () {
        assert_unchanged_via_json(val)
    }
}
