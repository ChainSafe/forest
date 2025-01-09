// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::{lotus_json_with_self, LotusJson};
use crate::message::Message as _;
use crate::shim::executor::ApplyRet;
use crate::shim::{
    address::Address,
    clock::ChainEpoch,
    econ::TokenAmount,
    error::ExitCode,
    executor::Receipt,
    message::Message,
    state_tree::{ActorID, ActorState},
};
use cid::Cid;
use fvm_ipld_encoding::RawBytes;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ApiInvocResult {
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Message>")]
    pub msg: Message,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Cid>")]
    pub msg_cid: Cid,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Option<Receipt>>")]
    pub msg_rct: Option<Receipt>,
    pub error: String,
    pub duration: u64,
    pub gas_cost: MessageGasCost,
    pub execution_trace: Option<ExecutionTrace>,
}

lotus_json_with_self!(ApiInvocResult);

impl PartialEq for ApiInvocResult {
    /// Ignore [`Self::duration`] as it is implementation-dependent
    fn eq(&self, other: &Self) -> bool {
        self.msg == other.msg
            && self.msg_cid == other.msg_cid
            && self.msg_rct == other.msg_rct
            && self.error == other.error
            && self.gas_cost == other.gas_cost
            && self.execution_trace == other.execution_trace
    }
}

#[derive(Default, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct MessageGasCost {
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Option<Cid>>")]
    pub message: Option<Cid>,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub gas_used: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub base_fee_burn: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub over_estimation_burn: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub miner_penalty: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub miner_tip: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub refund: TokenAmount,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub total_cost: TokenAmount,
}

lotus_json_with_self!(MessageGasCost);

impl MessageGasCost {
    pub fn new(message: &Message, apply_ret: &ApplyRet) -> anyhow::Result<Self> {
        Ok(Self {
            message: Some(message.cid()),
            gas_used: TokenAmount::from_atto(apply_ret.msg_receipt().gas_used()),
            base_fee_burn: apply_ret.base_fee_burn(),
            over_estimation_burn: apply_ret.over_estimation_burn(),
            miner_penalty: apply_ret.penalty(),
            miner_tip: apply_ret.miner_tip(),
            refund: apply_ret.refund(),
            total_cost: message.required_funds() - &apply_ret.refund(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ExecutionTrace {
    pub msg: MessageTrace,
    pub msg_rct: ReturnTrace,
    pub invoked_actor: Option<ActorTrace>,
    pub gas_charges: Vec<GasTrace>,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Vec<ExecutionTrace>>")]
    pub subcalls: Vec<ExecutionTrace>,
}

impl ExecutionTrace {
    pub fn sum_gas(&self) -> GasTrace {
        let mut out: GasTrace = GasTrace::default();
        for gc in self.gas_charges.iter() {
            out.total_gas += gc.total_gas;
            out.compute_gas += gc.compute_gas;
            out.storage_gas += gc.storage_gas;
        }
        out
    }
}

lotus_json_with_self!(ExecutionTrace);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct MessageTrace {
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Address>")]
    pub from: Address,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Address>")]
    pub to: Address,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<TokenAmount>")]
    pub value: TokenAmount,
    pub method: u64,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<RawBytes>")]
    pub params: RawBytes,
    pub params_codec: u64,
    pub gas_limit: Option<u64>,
    pub read_only: Option<bool>,
}

lotus_json_with_self!(MessageTrace);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ActorTrace {
    pub id: ActorID,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<ActorState>")]
    pub state: ActorState,
}

lotus_json_with_self!(ActorTrace);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ReturnTrace {
    pub exit_code: ExitCode,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<RawBytes>")]
    pub r#return: RawBytes,
    pub return_codec: u64,
}

lotus_json_with_self!(ReturnTrace);

#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct GasTrace {
    pub name: String,
    #[serde(rename = "tg")]
    pub total_gas: u64,
    #[serde(rename = "cg")]
    pub compute_gas: u64,
    #[serde(rename = "sg")]
    pub storage_gas: u64,
    #[serde(rename = "tt")]
    pub time_taken: u64,
}

lotus_json_with_self!(GasTrace);

impl PartialEq for GasTrace {
    /// Ignore [`Self::total_gas`] as it is implementation-dependent
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.total_gas == other.total_gas
            && self.compute_gas == other.compute_gas
            && self.storage_gas == other.storage_gas
    }
}

#[derive(PartialEq, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct InvocResult {
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Message>")]
    pub msg: Message,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Option<Receipt>>")]
    pub msg_rct: Option<Receipt>,
    pub error: Option<String>,
}
lotus_json_with_self!(InvocResult);

impl InvocResult {
    pub fn new(msg: Message, ret: &ApplyRet) -> Self {
        Self {
            msg,
            msg_rct: Some(ret.msg_receipt()),
            error: ret.failure_info(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct SectorExpiration {
    pub on_time: ChainEpoch,
    pub early: ChainEpoch,
}
lotus_json_with_self!(SectorExpiration);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct SectorLocation {
    pub deadline: u64,
    pub partition: u64,
}
lotus_json_with_self!(SectorLocation);
