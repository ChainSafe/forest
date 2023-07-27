// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::interpreter::{InvocResult, MessageGasCost};

pub mod json {
    use crate::shim::executor::{Receipt, Trace};
    use crate::shim::message::Message;
    use cid::Cid;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::*;

    /// Wrapper for serializing and de-serializing an `InvocResult` from JSON.
    #[derive(Deserialize, Serialize, Debug)]
    #[serde(transparent)]
    pub struct InvocResultJson(#[serde(with = "self")] pub InvocResult);

    /// Wrapper for serializing a `InvocResult` reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct InvocResultRef<'a>(#[serde(with = "self")] pub &'a InvocResult);

    impl From<InvocResultJson> for InvocResult {
        fn from(wrapper: InvocResultJson) -> Self {
            wrapper.0
        }
    }

    impl From<InvocResult> for InvocResultJson {
        fn from(ir: InvocResult) -> Self {
            InvocResultJson(ir)
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        #[serde(default, with = "crate::json::cid")]
        msg_cid: Cid,
        #[serde(with = "crate::json::message::json")]
        msg: Message,
        #[serde(with = "crate::json::message_receipt::json")]
        #[serde(rename = "MsgRct")]
        msg_receipt: Receipt,
        #[serde(with = "crate::json::message_gas_cost::json")]
        gas_cost: MessageGasCost,
        #[serde(with = "crate::json::trace::json::opt")]
        execution_trace: Option<Trace>,
        error: String,
        duration: u64,
    }

    pub fn serialize<S>(ir: &InvocResult, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            msg_cid: ir.msg_cid,
            msg: ir.msg.clone(),
            msg_receipt: ir.msg_receipt.clone(),
            gas_cost: ir.gas_cost.clone(),
            execution_trace: ir.execution_trace.clone(),
            error: ir.error.clone(),
            duration: ir.duration,
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<InvocResult, D::Error>
    where
        D: Deserializer<'de>,
    {
        let ir: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(InvocResult {
            msg_cid: ir.msg_cid,
            msg: ir.msg,
            msg_receipt: ir.msg_receipt,
            gas_cost: ir.gas_cost,
            execution_trace: ir.execution_trace,
            error: ir.error,
            duration: ir.duration,
        })
    }

    pub mod vec {
        use crate::utils::json::GoVecVisitor;
        use serde::ser::SerializeSeq;

        use super::{InvocResult, *};

        /// Wrapper for serializing and de-serializing an `InvocResult` vector from JSON.
        #[derive(Deserialize, Serialize)]
        #[serde(transparent)]
        pub struct InvocResultJsonVec(#[serde(with = "self")] pub Vec<InvocResult>);

        /// Wrapper for serializing an `InvocResult` slice to JSON.
        #[derive(Serialize)]
        #[serde(transparent)]
        pub struct InvocResultSlice<'a>(#[serde(with = "self")] pub &'a [InvocResult]);

        pub fn serialize<S>(m: &[InvocResult], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut seq = serializer.serialize_seq(Some(m.len()))?;
            for e in m {
                seq.serialize_element(&InvocResultRef(e))?;
            }
            seq.end()
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<InvocResult>, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(GoVecVisitor::<InvocResult, InvocResultJson>::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::interpreter::{InvocResult, MessageGasCost};
    use crate::shim::executor::Receipt;
    use crate::shim::message::Message;
    use cid::Cid;

    use quickcheck_macros::quickcheck;

    use super::*;

    impl quickcheck::Arbitrary for InvocResult {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            Self {
                msg_cid: Cid::arbitrary(g),
                msg: Message::arbitrary(g),
                msg_receipt: Receipt::arbitrary(g),
                gas_cost: MessageGasCost::arbitrary(g),
                execution_trace: Option::arbitrary(g),
                error: String::arbitrary(g),
                duration: u64::arbitrary(g),
            }
        }
    }

    #[quickcheck]
    fn invoc_result_roundtrip(ir: InvocResult) {
        let serialized = crate::to_string_with!(&ir, json::serialize);
        let parsed: InvocResult = crate::from_str_with!(&serialized, json::deserialize);
        assert_eq!(ir, parsed);
    }
}
