// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use base64::{prelude::BASE64_STANDARD, Engine};
    use forest_shim::crypto::{Signature, SignatureType};
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

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
            sig_type: m.signature_type(),
            bytes: BASE64_STANDARD.encode(&m.bytes),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Signature, D::Error>
    where
        D: Deserializer<'de>,
    {
        let JsonHelper { sig_type, bytes } = Deserialize::deserialize(deserializer)?;
        Ok(Signature::new(
            sig_type,
            BASE64_STANDARD.decode(bytes).map_err(de::Error::custom)?,
        ))
    }

    pub mod opt {
        use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

        use super::{Signature, SignatureJson, SignatureJsonRef};

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
        use serde::{Deserialize, Deserializer, Serialize, Serializer};

        use super::*;

        #[derive(Debug, Deserialize, Serialize)]
        #[serde(rename_all = "lowercase")]
        enum JsonHelperEnum {
            Bls,
            Secp256k1,
            Delegated,
        }

        #[derive(Debug, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct SignatureTypeJson(#[serde(with = "self")] pub SignatureType);

        pub fn serialize<S>(m: &SignatureType, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let json = match *m {
                SignatureType::BLS => JsonHelperEnum::Bls,
                SignatureType::Secp256k1 => JsonHelperEnum::Secp256k1,
                SignatureType::Delegated => JsonHelperEnum::Delegated,
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
                JsonHelperEnum::Delegated => SignatureType::Delegated,
            };
            Ok(signature_type)
        }
    }
}

#[cfg(test)]
mod tests {
    use forest_shim::crypto::{Signature, SignatureType};
    use quickcheck_macros::quickcheck;
    use serde_json;

    use super::json::{signature_type::SignatureTypeJson, SignatureJson, SignatureJsonRef};

    #[quickcheck]
    fn signature_roundtrip(signature: Signature) {
        let serialized = serde_json::to_string(&SignatureJsonRef(&signature)).unwrap();
        let parsed: SignatureJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(signature, parsed.0);
    }

    #[quickcheck]
    fn signaturetype_roundtrip(sigtype: SignatureType) {
        let serialized = serde_json::to_string(&SignatureTypeJson(sigtype)).unwrap();
        let parsed: SignatureTypeJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(sigtype, parsed.0);
    }
}
