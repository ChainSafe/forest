// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

impl<T> HasLotusJson for Option<T>
where
    T: HasLotusJson,
{
    type LotusJson = Option<T::LotusJson>;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        unimplemented!("only Option<Cid> is tested, below")
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.map(T::into_lotus_json)
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        lotus_json.map(T::from_lotus_json)
    }
}

#[test]
fn shapshots() {
    assert_one_snapshot(json!({"/": "baeaaaaa"}), Some(::cid::Cid::default()));
    assert_one_snapshot(json!(null), None::<::cid::Cid>);
}

#[cfg(test)]
quickcheck! {
    fn quickcheck(val: Option<::cid::Cid>) -> () {
        assert_unchanged_via_json(val)
    }
}
