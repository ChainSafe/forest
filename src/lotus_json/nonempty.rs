// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

impl<T> HasLotusJson for nunny::Vec<T>
where
    T: HasLotusJson,
{
    type LotusJson = nunny::Vec<<T as HasLotusJson>::LotusJson>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        unimplemented!("only NonEmpty<Cid> is tested, below")
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.into_iter_ne()
            .map(HasLotusJson::into_lotus_json)
            .collect_vec()
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        lotus_json
            .into_iter_ne()
            .map(HasLotusJson::from_lotus_json)
            .collect_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::cid::Cid;
    use nunny::vec as nonempty;
    use quickcheck_macros::quickcheck;

    #[test]
    fn shapshots() {
        assert_one_snapshot(json!([{"/": "baeaaaaa"}]), nonempty![::cid::Cid::default()]);
    }

    #[quickcheck]
    fn assert_unchanged(it: nunny::Vec<Cid>) {
        assert_unchanged_via_json(it)
    }
}
