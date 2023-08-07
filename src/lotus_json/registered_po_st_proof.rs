use crate::shim::sector::RegisteredPoStProof;
use fvm_shared3::sector::RegisteredPoStProof as RegisteredPoStProofV3;

use super::*;

#[derive(Serialize, Deserialize)]
pub struct RegisteredPoStProofLotusJson(i64);

impl HasLotusJson for RegisteredPoStProof {
    type LotusJson = RegisteredPoStProofLotusJson;
}

impl From<RegisteredPoStProof> for RegisteredPoStProofLotusJson {
    fn from(value: RegisteredPoStProof) -> Self {
        Self(i64::from(RegisteredPoStProofV3::from(value)))
    }
}

impl From<RegisteredPoStProofLotusJson> for RegisteredPoStProof {
    fn from(value: RegisteredPoStProofLotusJson) -> Self {
        Self::from(RegisteredPoStProofV3::from(value.0))
    }
}

#[test]
fn test() {
    assert_snapshot(
        json!(0),
        RegisteredPoStProof::from(RegisteredPoStProofV3::StackedDRGWinning2KiBV1),
    );
}

#[cfg(test)]
quickcheck! {
    fn round_trip(val: RegisteredPoStProof) -> bool {
        assert_via_json(val);
        true
    }
}
