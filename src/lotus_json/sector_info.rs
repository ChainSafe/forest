use super::*;
use crate::shim::sector::SectorInfo;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SectorInfoLotusJson {
    seal_proof: RegisteredSealProofLotusJson,
    sector_number: u64,
    sealed_c_i_d: CidLotusJson,
}

impl HasLotusJson for SectorInfo {
    type LotusJson = SectorInfoLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "SealProof": 0,
                "SectorNumber": 0,
                "SealedCID": {
                    "/": "baeaaaaa"
                }
            }),
            Self::new(
                fvm_shared3::sector::RegisteredSealProof::StackedDRG2KiBV1,
                0,
                ::cid::Cid::default(),
            ),
        )]
    }
}

impl From<SectorInfoLotusJson> for SectorInfo {
    fn from(value: SectorInfoLotusJson) -> Self {
        let SectorInfoLotusJson {
            seal_proof,
            sector_number,
            sealed_c_i_d,
        } = value;
        Self::new(
            crate::shim::sector::RegisteredSealProof::from(seal_proof).into(),
            sector_number,
            sealed_c_i_d.into(),
        )
    }
}

impl From<SectorInfo> for SectorInfoLotusJson {
    fn from(value: SectorInfo) -> Self {
        let fvm_shared3::sector::SectorInfo {
            proof,
            sector_number,
            sealed_cid,
        } = From::from(value);
        Self {
            seal_proof: crate::shim::sector::RegisteredSealProof::from(proof).into(),
            sector_number,
            sealed_c_i_d: sealed_cid.into(),
        }
    }
}
