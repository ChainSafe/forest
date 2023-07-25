// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use crate::shim::address::Address;
    use crate::shim::econ::TokenAmount;
    use crate::shim::executor::TraceMessage;
    use crate::shim::message::MethodNum;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing and de-serializing a TraceMessage from JSON.
    #[derive(Deserialize, Serialize, Debug)]
    #[serde(transparent)]
    pub struct TraceMessageJson(#[serde(with = "self")] pub TraceMessage);

    impl From<TraceMessageJson> for TraceMessage {
        fn from(wrapper: TraceMessageJson) -> Self {
            wrapper.0
        }
    }

    impl From<TraceMessage> for TraceMessageJson {
        fn from(wrapper: TraceMessage) -> Self {
            TraceMessageJson(wrapper)
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

    pub fn serialize<S>(m: &TraceMessage, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            from: m.from,
            to: m.to,
            value: m.value.clone(),
            method_num: m.method_num,
            params: m.params.clone(),
            params_codec: m.params_codec,
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<TraceMessage, D::Error>
    where
        D: Deserializer<'de>,
    {
        let m: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(TraceMessage {
            from: m.from,
            to: m.to,
            value: m.value,
            method_num: m.method_num,
            params: m.params,
            params_codec: m.params_codec,
        })
    }
}

#[cfg(test)]
pub mod tests {}
