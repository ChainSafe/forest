use crate::blocks::ElectionProof;

use super::*;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ElectionProofLotusJson {
    #[serde(rename = "VRFProof")]
    vrfproof: VRFProofLotusJson,
    win_count: i64,
}

impl HasLotusJson for ElectionProof {
    type LotusJson = ElectionProofLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "WinCount": 0,
                "VRFProof": ""
            }),
            ElectionProof::default(),
        )]
    }
}

impl From<ElectionProofLotusJson> for ElectionProof {
    fn from(value: ElectionProofLotusJson) -> Self {
        let ElectionProofLotusJson {
            vrfproof,
            win_count,
        } = value;
        Self {
            win_count,
            vrfproof: vrfproof.into(),
        }
    }
}

impl From<ElectionProof> for ElectionProofLotusJson {
    fn from(value: ElectionProof) -> Self {
        let ElectionProof {
            win_count,
            vrfproof,
        } = value;
        Self {
            vrfproof: vrfproof.into(),
            win_count,
        }
    }
}

#[cfg(test)]
quickcheck! {
    fn round_trip(val: ElectionProof) -> bool {
        assert_via_json(val);
        true
    }
}
