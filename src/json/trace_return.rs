// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use base64::{prelude::BASE64_STANDARD, Engine};
    use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};

    //use crate::json::address::json::AddressJson;
    use crate::shim::executor::TraceReturn;
    use fvm_shared3::error::ExitCode;

    /// Wrapper for serializing and de-serializing a TraceReturn from JSON.
    #[derive(Deserialize, Serialize, Debug)]
    #[serde(transparent)]
    pub struct TraceReturnJson(#[serde(with = "self")] pub TraceReturn);

    impl From<TraceReturnJson> for TraceReturn {
        fn from(wrapper: TraceReturnJson) -> Self {
            wrapper.0
        }
    }

    impl From<TraceReturn> for TraceReturnJson {
        fn from(wrapper: TraceReturn) -> Self {
            TraceReturnJson(wrapper)
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        exit_code: ExitCode,
        #[serde(rename = "Return")]
        return_data: String,
        return_codec: u64,
    }

    pub fn serialize<S>(t: &TraceReturn, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            exit_code: t.exit_code,
            return_data: BASE64_STANDARD.encode(&t.return_data),
            return_codec: t.return_codec,
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<TraceReturn, D::Error>
    where
        D: Deserializer<'de>,
    {
        let m: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(TraceReturn {
            exit_code: m.exit_code,
            return_data: m.return_data.into(),
            return_codec: m.return_codec,
        })
    }
}

#[cfg(test)]
pub mod tests {}
