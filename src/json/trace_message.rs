// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use crate::shim::address::Address;
    use crate::shim::econ::TokenAmount;
    use crate::shim::executor::TraceMessage;
    use crate::shim::message::MethodNum;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing and de-serializing a `TraceMessage` from JSON.
    #[derive(Deserialize, Serialize, Debug)]
    #[serde(transparent)]
    pub struct TraceMessageJson(#[serde(with = "self")] pub TraceMessage);

    /// Wrapper for serializing a `TraceMessage` reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct TraceMessageJsonRef<'a>(#[serde(with = "self")] pub &'a TraceMessage);

    impl From<TraceMessageJson> for TraceMessage {
        fn from(wrapper: TraceMessageJson) -> Self {
            wrapper.0
        }
    }

    impl From<TraceMessage> for TraceMessageJson {
        fn from(tm: TraceMessage) -> Self {
            TraceMessageJson(tm)
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        #[serde(with = "crate::json::address::json")]
        pub from: Address,
        #[serde(with = "crate::json::address::json")]
        pub to: Address,
        #[serde(with = "crate::json::token_amount::json")]
        pub value: TokenAmount,
        #[serde(rename = "Method")]
        pub method_num: MethodNum,
        #[serde(with = "crate::json::bytes::json")]
        pub params: Vec<u8>,
        pub params_codec: u64,
    }

    pub fn serialize<S>(tm: &TraceMessage, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            from: tm.from,
            to: tm.to,
            value: tm.value.clone(),
            method_num: tm.method_num,
            params: tm.params.clone(),
            params_codec: tm.params_codec,
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<TraceMessage, D::Error>
    where
        D: Deserializer<'de>,
    {
        let h: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(TraceMessage {
            from: h.from,
            to: h.to,
            value: h.value,
            method_num: h.method_num,
            params: h.params,
            params_codec: h.params_codec,
        })
    }
}

#[cfg(test)]
pub mod tests {
    use crate::shim::executor::TraceMessage;
    use quickcheck_macros::quickcheck;

    use super::json::{TraceMessageJson, TraceMessageJsonRef};

    #[quickcheck]
    fn trace_message_roundtrip(tm: TraceMessage) {
        let serialized = serde_json::to_string(&TraceMessageJsonRef(&tm)).unwrap();
        let parsed: TraceMessageJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(tm, parsed.0);
    }
}
