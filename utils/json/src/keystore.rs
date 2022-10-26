pub mod json {
    use crate::signature::json::signature_type::SignatureTypeJson;
    use forest_key_management::KeyInfo;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing and de-serializing a `KeyInfo` from JSON.
    #[derive(Clone, Deserialize, Serialize, Debug, PartialEq, Eq)]
    #[serde(transparent)]
    pub struct KeyInfoJson(#[serde(with = "self")] pub KeyInfo);

    /// Wrapper for serializing a `KeyInfo` reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct KeyInfoJsonRef<'a>(#[serde(with = "self")] pub &'a KeyInfo);

    impl From<KeyInfoJson> for KeyInfo {
        fn from(key: KeyInfoJson) -> KeyInfo {
            key.0
        }
    }
    #[derive(Serialize, Deserialize)]
    struct JsonHelper {
        #[serde(rename = "Type")]
        sig_type: SignatureTypeJson,
        #[serde(rename = "PrivateKey")]
        private_key: String,
    }

    pub fn serialize<S>(k: &KeyInfo, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            sig_type: SignatureTypeJson(*k.key_type()),
            private_key: base64::encode(k.private_key()),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<KeyInfo, D::Error>
    where
        D: Deserializer<'de>,
    {
        let JsonHelper {
            sig_type,
            private_key,
        } = Deserialize::deserialize(deserializer)?;
        Ok(KeyInfo::new(
            sig_type.0,
            base64::decode(private_key).map_err(de::Error::custom)?,
        ))
    }
}
