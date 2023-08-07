use super::*;
use crate::shim::crypto::Signature;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SignatureLotusJson {
    r#type: SignatureTypeLotusJson,
    #[serde(with = "base64_standard")]
    data: Vec<u8>,
}

impl HasLotusJson for Signature {
    type LotusJson = SignatureLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({"Type": "bls", "Data": "aGVsbG8gd29ybGQh"}),
            Signature {
                sig_type: crate::shim::crypto::SignatureType::Bls,
                bytes: Vec::from_iter(*b"hello world!"),
            },
        )]
    }
}

impl From<SignatureLotusJson> for Signature {
    fn from(value: SignatureLotusJson) -> Self {
        let SignatureLotusJson { r#type, data } = value;
        Self {
            sig_type: r#type.into(),
            bytes: data,
        }
    }
}

impl From<Signature> for SignatureLotusJson {
    fn from(value: Signature) -> Self {
        let Signature { sig_type, bytes } = value;
        Self {
            r#type: sig_type.into(),
            data: bytes,
        }
    }
}

#[cfg(test)]
quickcheck! {
    fn round_trip(val: Signature) -> bool {
        assert_via_json(val);
        true
    }
}
