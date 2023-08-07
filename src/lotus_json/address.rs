use super::*;

use crate::shim::address::Address;

#[derive(Serialize, Deserialize, From, Into)]
#[serde(transparent)]
pub struct AddressLotusJson(#[serde(with = "stringify")] Address);

impl HasLotusJson for Address {
    type LotusJson = AddressLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("f00"), Address::default())]
    }
}
