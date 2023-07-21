// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use crate::shim::executor::Trace;
    use crate::shim::executor::TraceMessage;
    use crate::shim::executor::TraceReturn;

    use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing and de-serializing a Trace from JSON.
    #[derive(Deserialize, Serialize, Debug)]
    #[serde(transparent)]
    pub struct TraceJson(#[serde(with = "self")] pub Trace);

    impl From<TraceJson> for Trace {
        fn from(wrapper: TraceJson) -> Self {
            wrapper.0
        }
    }

    impl From<Trace> for TraceJson {
        fn from(wrapper: Trace) -> Self {
            TraceJson(wrapper)
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        #[serde(with = "crate::json::trace_message::json")]
        msg: TraceMessage,
        // #[serde(with = "crate::json::trace_return::json")]
        // msg_ret: TraceReturn,
        // gas_charges: Vec<TraceGasCharge>,
        // subcalls: Vec<Trace>,
    }

    pub fn serialize<S>(t: &Trace, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            msg: t.msg.clone(),
            //msg_ret: t.msg_ret.clone(),
        }.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Trace, D::Error>
    where
        D: Deserializer<'de>,
    {
        let m: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(Trace {
            msg: m.msg,
            //msg_ret: m.msg_ret,
            // gas_charges: m.gas_charges,
            // subcalls: m.subcalls,
        })
    }
}

#[cfg(test)]
pub mod tests {
}
