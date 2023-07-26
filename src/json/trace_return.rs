// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::shim::executor::TraceReturn;
    use fvm_shared3::error::ExitCode;

    /// Wrapper for serializing and de-serializing a `TraceReturn` from JSON.
    #[derive(Deserialize, Serialize, Debug)]
    #[serde(transparent)]
    pub struct TraceReturnJson(#[serde(with = "self")] pub TraceReturn);

    impl From<TraceReturnJson> for TraceReturn {
        fn from(wrapper: TraceReturnJson) -> Self {
            wrapper.0
        }
    }

    impl From<TraceReturn> for TraceReturnJson {
        fn from(tr: TraceReturn) -> Self {
            TraceReturnJson(tr)
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        exit_code: ExitCode,
        #[serde(rename = "Return")]
        #[serde(with = "crate::json::bytes::json")]
        return_data: Vec<u8>,
        return_codec: u64,
    }

    pub fn serialize<S>(tr: &TraceReturn, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            exit_code: tr.exit_code,
            return_data: tr.return_data.clone(),
            return_codec: tr.return_codec,
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<TraceReturn, D::Error>
    where
        D: Deserializer<'de>,
    {
        let h: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(TraceReturn {
            exit_code: h.exit_code,
            return_data: h.return_data,
            return_codec: h.return_codec,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::shim::executor::TraceReturn;
    use quickcheck_macros::quickcheck;

    use super::*;

    impl quickcheck::Arbitrary for TraceReturn {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            Self {
                exit_code: u32::arbitrary(g).into(),
                return_data: Vec::arbitrary(g),
                return_codec: u64::arbitrary(g),
            }
        }
    }

    #[quickcheck]
    fn trace_return_roundtrip(tr: TraceReturn) {
        let serialized = crate::to_string_with!(&tr, json::serialize);
        let parsed: TraceReturn = crate::from_str_with!(&serialized, json::deserialize);
        assert_eq!(tr, parsed);
    }
}
