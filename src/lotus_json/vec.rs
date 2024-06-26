// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

impl<T> HasLotusJson for Vec<T>
// TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/4032
//                  This shouldn't recurse - LotusJson<Vec<T>> should only handle
//                  the OUTER issue of serializing an empty Vec as null, and
//                  shouldn't be interested in the inner representation.
where
    T: HasLotusJson + Clone,
{
    type LotusJson = Option<Vec<T::LotusJson>>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        unimplemented!("only Vec<Cid> is tested, below")
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        match self.is_empty() {
            true => None,
            false => Some(self.into_iter().map(T::into_lotus_json).collect()),
        }
    }

    fn from_lotus_json(it: Self::LotusJson) -> Self {
        match it {
            Some(it) => it.into_iter().map(T::from_lotus_json).collect(),
            None => vec![],
        }
    }
}

#[test]
fn snapshots() {
    assert_one_snapshot(json!([{"/": "baeaaaaa"}]), vec![::cid::Cid::default()]);
}

#[cfg(test)]
quickcheck! {
    fn quickcheck(val: Vec<::cid::Cid>) -> () {
        assert_unchanged_via_json(val)
    }
}
