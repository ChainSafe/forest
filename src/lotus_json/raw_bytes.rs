use super::*;
use fvm_ipld_encoding::RawBytes;

#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct RawBytesLotusJson(#[serde(with = "base64_standard")] Vec<u8>);

impl HasLotusJson for RawBytes {
    type LotusJson = RawBytesLotusJson;
}

impl From<RawBytes> for RawBytesLotusJson {
    fn from(value: RawBytes) -> Self {
        RawBytesLotusJson(Vec::from(value))
    }
}

impl From<RawBytesLotusJson> for RawBytes {
    fn from(value: RawBytesLotusJson) -> Self {
        Self::from(value.0)
    }
}

#[test]
fn test() {
    assert_snapshot(
        json!("aGVsbG8gd29ybGQh"),
        RawBytes::new(Vec::from_iter(*b"hello world!")),
    );
}

#[cfg(test)]
quickcheck! {
    fn round_trip(val: Vec<u8>) -> bool {
        assert_via_json(RawBytes::new(val));
        true
    }
}
