// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use crate::shim::executor::ExecutionEvent;
    use crate::shim::gas::GasCharge;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::*;

    /// Wrapper for serializing and de-serializing an `ExecutionEvent` from JSON.
    #[derive(Deserialize, Serialize, Debug)]
    #[serde(transparent)]
    pub struct ExecutionEventJson(#[serde(with = "self")] pub ExecutionEvent);

    /// Wrapper for serializing a `ExecutionEvent` reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct ExecutionEventRef<'a>(#[serde(with = "self")] pub &'a ExecutionEvent);

    impl From<ExecutionEventJson> for ExecutionEvent {
        fn from(wrapper: ExecutionEventJson) -> Self {
            wrapper.0
        }
    }

    impl From<ExecutionEvent> for ExecutionEventJson {
        fn from(ir: ExecutionEvent) -> Self {
            ExecutionEventJson(ir)
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub enum JsonHelper {
        #[serde(with = "crate::json::gas_charge::json")]
        GasCharge(GasCharge),
        // TODO:
        // Call {
        //     from: ActorID,
        //     to: Address,
        //     method: MethodNum,
        //     params: Option<IpldBlock>,
        //     value: TokenAmount,
        // },
        // CallReturn(ExitCode, Option<IpldBlock>),
        // CallError(SyscallError),
    }

    pub fn serialize<S>(ev: &ExecutionEvent, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match ev {
            ExecutionEvent::V2(v2) => todo!(),
            ExecutionEvent::V3(v3) => todo!(),
        }
        //JsonHelper::GasCharge()
        // JsonHelper {
        //     msg_cid: ir.msg_cid,
        //     msg: ir.msg.clone(),
        //     msg_receipt: ir.msg_receipt.clone(),
        //     gas_cost: ir.gas_cost.clone(),
        //     error: ir.error.clone(),
        // }
        // .serialize(serializer)
        //todo!()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ExecutionEvent, D::Error>
    where
        D: Deserializer<'de>,
    {
        // let ir: JsonHelper = Deserialize::deserialize(deserializer)?;
        // Ok(ExecutionEvent {
        //     msg_cid: ir.msg_cid,
        //     msg: ir.msg,
        //     msg_receipt: ir.msg_receipt,
        //     gas_cost: ir.gas_cost,
        //     //exec_trace: ir.exec_trace,
        //     error: ir.error,
        // })
        todo!()
    }
}

#[cfg(test)]
pub mod tests {
    // todo!
}
