// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use ::nonempty::NonEmpty;

impl<T> HasLotusJson for NonEmpty<T>
where
    T: HasLotusJson,
{
    type LotusJson = NonEmpty<<T as HasLotusJson>::LotusJson>;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        unimplemented!("only NonEmpty<Cid> is tested, below")
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.map(HasLotusJson::into_lotus_json)
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        lotus_json.map(HasLotusJson::from_lotus_json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::cid::Cid;
    use ::nonempty::nonempty;
    use quickcheck_macros::quickcheck;

    #[test]
    fn shapshots() {
        assert_one_snapshot(json!([{"/": "baeaaaaa"}]), nonempty![::cid::Cid::default()]);
    }

    #[quickcheck]
    fn assert_unchanged(head: Cid, tail: Vec<Cid>) {
        assert_unchanged_via_json(NonEmpty::from((head, tail)))
    }
}
