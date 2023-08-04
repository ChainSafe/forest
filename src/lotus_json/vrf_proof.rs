use crate::json::vrf::VRFProof;

use super::*;

#[derive(Serialize, Deserialize)]
pub struct VRFProofLotusJson(VecU8LotusJson);

impl HasLotusJson for VRFProof {
    type LotusJson = VRFProofLotusJson;
}

impl From<VRFProofLotusJson> for VRFProof {
    fn from(value: VRFProofLotusJson) -> Self {
        Self(value.0.into())
    }
}

impl From<VRFProof> for VRFProofLotusJson {
    fn from(value: VRFProof) -> Self {
        Self(value.0.into())
    }
}

#[test]
fn test() {
    assert_snapshot(
        json!("aGVsbG8gd29ybGQh"),
        VRFProof(Vec::from_iter(*b"hello world!")),
    );
}

#[cfg(test)]
quickcheck! {
    fn round_trip(val: Vec<u8>) -> bool {
        assert_via_json(VRFProof(val));
        true
    }
}
