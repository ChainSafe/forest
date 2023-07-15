// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::interpreter::{InvocResult, MessageGasCost};

pub mod json {
    use crate::shim::executor::Receipt;
    use crate::shim::executor::Trace;
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
        msg_receipt: Receipt,
        #[serde(with = "crate::json::message_gas_cost::json")]
        gas_cost: MessageGasCost,
        execution_trace: Option<Trace>,
        error: String,
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
pub mod tests {
    // todo!
}
