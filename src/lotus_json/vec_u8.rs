use super::*;

#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct VecU8LotusJson(#[serde(with = "base64_standard")] Vec<u8>);

impl HasLotusJson for Vec<u8> {
    type LotusJson = VecU8LotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(json!("aGVsbG8gd29ybGQh"), Vec::from_iter(*b"hello world!"))]
    }
}

impl From<Vec<u8>> for VecU8LotusJson {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}

impl From<VecU8LotusJson> for Vec<u8> {
    fn from(value: VecU8LotusJson) -> Self {
        value.0
    }
}

#[cfg(test)]
quickcheck! {
    fn round_trip(val: Vec<u8>) -> bool {
        assert_via_json(val);
        true
    }
}
