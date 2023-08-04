use super::*;
use crate::shim::crypto::SignatureType;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SignatureTypeLotusJson {
    Bls,
    Secp256k1,
    Delegated,
}

impl HasLotusJson for SignatureType {
    type LotusJson = SignatureTypeLotusJson;
}

impl From<SignatureTypeLotusJson> for SignatureType {
    fn from(value: SignatureTypeLotusJson) -> Self {
        match value {
            SignatureTypeLotusJson::Bls => Self::Bls,
            SignatureTypeLotusJson::Secp256k1 => Self::Secp256k1,
            SignatureTypeLotusJson::Delegated => Self::Delegated,
        }
    }
}

impl From<SignatureType> for SignatureTypeLotusJson {
    fn from(value: SignatureType) -> Self {
        match value {
            SignatureType::Secp256k1 => Self::Secp256k1,
            SignatureType::Bls => Self::Bls,
            SignatureType::Delegated => Self::Delegated,
        }
    }
}

#[test]
fn test() {
    assert_snapshot(json!("bls"), SignatureType::Bls);
}

#[cfg(test)]
quickcheck! {
    fn round_trip(val: SignatureType) -> bool {
        assert_via_json(val);
        true
    }
}
