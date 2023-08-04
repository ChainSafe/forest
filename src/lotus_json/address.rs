use super::*;

use crate::shim::address::Address;

#[derive(Serialize, Deserialize, From, Into)]
#[serde(transparent)]
pub struct AddressLotusJson(#[serde(with = "stringify")] Address);

impl HasLotusJson for Address {
    type LotusJson = AddressLotusJson;
}

#[test]
fn test() {
    assert_snapshot(json!("f00"), Address::default());
}

#[cfg(test)]
quickcheck! {
    fn round_trip(val: Address) -> bool {
        assert_via_json(val);
        true
    }
}
