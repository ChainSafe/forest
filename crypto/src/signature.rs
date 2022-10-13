// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use bls_signatures::{
    verify_messages, PublicKey as BlsPubKey, Serialize, Signature as BlsSignature,
};
use fvm_shared::address::Address;

/// Returns `String` error if a BLS signature is invalid.
pub(crate) fn verify_bls_sig(signature: &[u8], data: &[u8], addr: &Address) -> Result<(), String> {
    let pub_k = addr.payload_bytes();

    // generate public key object from bytes
    let pk = BlsPubKey::from_bytes(&pub_k).map_err(|e| e.to_string())?;

    // generate signature struct from bytes
    let sig = BlsSignature::from_bytes(signature).map_err(|e| e.to_string())?;

    // BLS verify hash against key
    if verify_messages(&sig, &[data], &[pk]) {
        Ok(())
    } else {
        Err(format!(
            "bls signature verification failed for addr: {}",
            addr
        ))
    }
}

pub mod json {
    use forest_encoding::de;
    use fvm_shared::crypto::signature::{Signature, SignatureType};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    // Wrapper for serializing and deserializing a Signature from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct SignatureJson(#[serde(with = "self")] pub Signature);

    /// Wrapper for serializing a Signature reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct SignatureJsonRef<'a>(#[serde(with = "self")] pub &'a Signature);

    #[derive(Serialize, Deserialize)]
    struct JsonHelper {
        #[serde(rename = "Type")]
        sig_type: SignatureType,
        #[serde(rename = "Data")]
        bytes: String,
    }

    pub fn serialize<S>(m: &Signature, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            sig_type: m.sig_type,
            bytes: base64::encode(&m.bytes),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Signature, D::Error>
    where
        D: Deserializer<'de>,
    {
        let JsonHelper { sig_type, bytes } = Deserialize::deserialize(deserializer)?;
        Ok(Signature {
            sig_type,
            bytes: base64::decode(bytes).map_err(de::Error::custom)?,
        })
    }

    pub mod opt {
        use super::{Signature, SignatureJson, SignatureJsonRef};
        use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

        pub fn serialize<S>(v: &Option<Signature>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            v.as_ref().map(SignatureJsonRef).serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Signature>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s: Option<SignatureJson> = Deserialize::deserialize(deserializer)?;
            Ok(s.map(|v| v.0))
        }
    }

    pub mod signature_type {
        use super::*;
        use serde::{Deserialize, Deserializer, Serialize, Serializer};

        #[derive(Debug, Deserialize, Serialize)]
        #[serde(rename_all = "lowercase")]
        enum JsonHelperEnum {
            Bls,
            Secp256k1,
        }

        #[derive(Debug, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct SignatureTypeJson(#[serde(with = "self")] pub SignatureType);

        pub fn serialize<S>(m: &SignatureType, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let json = match m {
                SignatureType::BLS => JsonHelperEnum::Bls,
                SignatureType::Secp256k1 => JsonHelperEnum::Secp256k1,
            };
            json.serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<SignatureType, D::Error>
        where
            D: Deserializer<'de>,
        {
            let json_enum: JsonHelperEnum = Deserialize::deserialize(deserializer)?;

            let signature_type = match json_enum {
                JsonHelperEnum::Bls => SignatureType::BLS,
                JsonHelperEnum::Secp256k1 => SignatureType::Secp256k1,
            };
            Ok(signature_type)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::json::signature_type::SignatureTypeJson;
    use super::json::{SignatureJson, SignatureJsonRef};
    use fvm_shared::crypto::signature::{Signature, SignatureType};
    use quickcheck_macros::quickcheck;
    use serde_json;

    #[derive(Clone, Debug, PartialEq)]
    struct SignatureWrapper {
        signature: Signature,
    }

    #[derive(Clone, Debug, PartialEq)]
    struct SignatureTypeWrapper {
        sigtype: SignatureType,
    }

    impl quickcheck::Arbitrary for SignatureWrapper {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let sigtype = SignatureTypeWrapper::arbitrary(g);
            let signature = Signature {
                bytes: Vec::arbitrary(g),
                sig_type: sigtype.sigtype,
            };
            SignatureWrapper { signature }
        }
    }

    impl quickcheck::Arbitrary for SignatureTypeWrapper {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let sigtype = g
                .choose(&[SignatureType::Secp256k1, SignatureType::BLS])
                .unwrap();
            SignatureTypeWrapper { sigtype: *sigtype }
        }
    }

    #[quickcheck]
    fn signature_roundtrip(signature: SignatureWrapper) {
        let serialized = serde_json::to_string(&SignatureJsonRef(&signature.signature)).unwrap();
        let parsed: SignatureJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(signature.signature, parsed.0);
    }

    #[quickcheck]
    fn signaturetype_roundtrip(sigtype: SignatureTypeWrapper) {
        let serialized = serde_json::to_string(&SignatureTypeJson(sigtype.sigtype)).unwrap();
        let parsed: SignatureTypeJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(sigtype.sigtype, parsed.0);
    }
}
