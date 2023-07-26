// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use crate::shim::executor::Trace;
    use crate::shim::executor::TraceGasCharge;
    use crate::shim::executor::TraceMessage;
    use crate::shim::executor::TraceReturn;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing and de-serializing a `Trace` from JSON.
    #[derive(Deserialize, Serialize, Debug)]
    #[serde(transparent)]
    pub struct TraceJson(#[serde(with = "self")] pub Trace);

    /// Wrapper for serializing a `Trace` reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct TraceJsonRef<'a>(#[serde(with = "self")] pub &'a Trace);

    impl From<TraceJson> for Trace {
        fn from(wrapper: TraceJson) -> Self {
            wrapper.0
        }
    }

    impl From<Trace> for TraceJson {
        fn from(t: Trace) -> Self {
            TraceJson(t)
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        #[serde(with = "crate::json::trace_message::json")]
        msg: TraceMessage,
        #[serde(with = "crate::json::trace_return::json")]
        #[serde(rename = "MsgRct")]
        msg_ret: TraceReturn,
        #[serde(with = "crate::json::trace_gas_charge::json::vec")]
        gas_charges: Vec<TraceGasCharge>,
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
            gas_charges: t.gas_charges.clone(),
            subcalls: t.subcalls.clone(),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Trace, D::Error>
    where
        D: Deserializer<'de>,
    {
        let h: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(Trace {
            msg: h.msg,
            msg_ret: h.msg_ret,
            gas_charges: h.gas_charges,
            subcalls: h.subcalls,
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

    pub mod opt {
        use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

        use super::{Trace, TraceJson, TraceJsonRef};

        pub fn serialize<S>(v: &Option<Trace>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            v.as_ref().map(TraceJsonRef).serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Trace>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s: Option<TraceJson> = Deserialize::deserialize(deserializer)?;
            Ok(s.map(|v| v.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::shim::executor::Trace;
    use crate::shim::executor::TraceMessage;
    use crate::shim::executor::TraceReturn;
    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;

    use super::*;

    impl quickcheck::Arbitrary for Trace {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            Self::arbitrary_trace(g, &mut g.size())
        }
    }

    impl Trace {
        /// Special version on `arbitrary` to battle possible recursion
        fn arbitrary_trace(g: &mut quickcheck::Gen, size: &mut usize) -> Self {
            if *size == 0 {
                return Trace::default();
            }
            // Just decrementing size produced a recursion limit exceeded error
            *size = size.saturating_sub(10);
            Self {
                msg: TraceMessage::arbitrary(g),
                msg_ret: TraceReturn::arbitrary(g),
                gas_charges: Vec::arbitrary(g),
                subcalls: (0..Self::arbitrary_size(g, size))
                    .map(|_| Self::arbitrary_trace(g, size))
                    .collect(),
            }
        }

        fn arbitrary_size(g: &mut quickcheck::Gen, size: &mut usize) -> usize {
            if *size == 0 {
                return 0;
            }
            usize::arbitrary(g) % *size
        }
    }

    #[quickcheck]
    fn trace_roundtrip(t: Trace) {
        let serialized = crate::to_string_with!(&t, json::serialize);
        let parsed: Trace = crate::from_str_with!(&serialized, json::deserialize);
        assert_eq!(t, parsed);
    }
}
