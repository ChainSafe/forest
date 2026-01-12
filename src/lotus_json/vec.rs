// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

impl<T> HasLotusJson for Vec<T>
// TODO(forest): https://github.com/ChainSafe/forest/issues/4032
//               This shouldn't recurse - LotusJson<Vec<T>> should only handle
//               the OUTER issue of serializing an empty Vec as null, and
//               shouldn't be interested in the inner representation.
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

// an empty `Vec<T>` serializes into `null` lotus json by default,
// while an empty `NotNullVec<T>` serializes into `[]`
// this is a temporary workaround and will likely be deprecated once
// other issues on serde of `Vec<T>` are resolved.
#[derive(Debug, Clone, PartialEq, JsonSchema)]
pub struct NotNullVec<T>(pub Vec<T>);

impl<T> HasLotusJson for NotNullVec<T>
where
    T: HasLotusJson + Clone,
{
    type LotusJson = Vec<T::LotusJson>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        unimplemented!("only Vec<Cid> is tested, below")
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.0.into_iter().map(T::into_lotus_json).collect()
    }

    fn from_lotus_json(it: Self::LotusJson) -> Self {
        Self(it.into_iter().map(T::from_lotus_json).collect())
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
