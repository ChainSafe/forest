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

    /// Wrapper for serializing a Trace reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct TraceJsonRef<'a>(#[serde(with = "self")] pub &'a Trace);

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
        #[serde(with = "crate::json::trace_return::json")]
        msg_ret: TraceReturn,
        // gas_charges: Vec<TraceGasCharge>,
        #[serde(with = "crate::json::trace::json::vec")]
        subcalls: Vec<Trace>,
    }

    pub fn serialize<S>(t: &Trace, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            msg: t.msg.clone(),
            msg_ret: t.msg_ret.clone(),
            subcalls: t.subcalls.clone(),
        }.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Trace, D::Error>
    where
        D: Deserializer<'de>,
    {
        let m: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(Trace {
            msg: m.msg,
            msg_ret: m.msg_ret,
            // gas_charges: m.gas_charges,
            subcalls: m.subcalls,
        })
    }

    pub mod vec {
        use crate::utils::json::GoVecVisitor;
        use serde::ser::SerializeSeq;

        use super::*;

        pub fn serialize<S>(m: &[Trace], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut seq = serializer.serialize_seq(Some(m.len()))?;
            for e in m {
                seq.serialize_element(&TraceJsonRef(e))?;
            }
            seq.end()
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Trace>, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(GoVecVisitor::<Trace, TraceJson>::new())
        }
    }
}

#[cfg(test)]
pub mod tests {
}
