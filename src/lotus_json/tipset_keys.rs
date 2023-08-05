use crate::blocks::TipsetKeys;

use super::*;

#[derive(Serialize, Deserialize)]
pub struct TipsetKeysLotusJson(VecLotusJson<CidLotusJson>);

impl HasLotusJson for TipsetKeys {
    type LotusJson = TipsetKeysLotusJson;
}

impl From<TipsetKeys> for TipsetKeysLotusJson {
    fn from(value: TipsetKeys) -> Self {
        let TipsetKeys { cids } = value;
        Self(cids.into())
    }
}

impl From<TipsetKeysLotusJson> for TipsetKeys {
    fn from(value: TipsetKeysLotusJson) -> Self {
        let TipsetKeysLotusJson(cids) = value;
        Self { cids: cids.into() }
    }
}

#[test]
fn test() {
    assert_snapshot(
        json!([{"/": "baeaaaaa"}]),
        TipsetKeys {
            cids: vec![::cid::Cid::default()],
        },
    );
}

#[cfg(test)]
quickcheck! {
    fn round_trip(val: TipsetKeys) -> bool {
        assert_via_json(val);
        true
    }
}
