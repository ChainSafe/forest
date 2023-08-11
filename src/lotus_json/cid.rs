// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

#[derive(Serialize, Deserialize)]
pub struct CidLotusJsonGeneric<const S: usize> {
    #[serde(rename = "/", with = "stringify")]
    slash: ::cid::CidGeneric<S>,
}

impl<const S: usize> HasLotusJson for ::cid::CidGeneric<S> {
    type LotusJson = CidLotusJsonGeneric<S>;

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
