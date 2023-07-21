// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::borrow::Borrow;
use std::borrow::Cow;

use anyhow::anyhow;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use fvm2::executor::ApplyRet as ApplyRet_v2;
use fvm3::executor::ApplyRet as ApplyRet_v3;
pub use fvm3::gas::GasCharge as GasChargeV3;
pub use fvm3::trace::ExecutionEvent as ExecutionEvent_v3;
use fvm_shared2::receipt::Receipt as Receipt_v2;
use fvm_shared3::error::ErrorNumber;
use fvm_shared3::error::ExitCode;
pub use fvm_shared3::receipt::Receipt as Receipt_v3;

use fvm_ipld_encoding::{ipld_block::IpldBlock, strict_bytes, RawBytes};

use crate::shim::address::Address;
use crate::shim::econ::TokenAmount;
use crate::shim::message::MethodNum;

#[derive(Clone, Debug)]
pub enum ApplyRet {
    V2(Box<ApplyRet_v2>),
    V3(Box<ApplyRet_v3>),
}

impl From<ApplyRet_v2> for ApplyRet {
    fn from(other: ApplyRet_v2) -> Self {
        ApplyRet::V2(Box::new(other))
    }
}

impl From<ApplyRet_v3> for ApplyRet {
    fn from(other: ApplyRet_v3) -> Self {
        ApplyRet::V3(Box::new(other))
    }
}

impl ApplyRet {
    pub fn failure_info(&self) -> Option<String> {
        match self {
            ApplyRet::V2(v2) => v2.failure_info.as_ref().map(|failure| failure.to_string()),
            ApplyRet::V3(v3) => v3.failure_info.as_ref().map(|failure| failure.to_string()),
        }
    }

    pub fn miner_tip(&self) -> TokenAmount {
        match self {
            ApplyRet::V2(v2) => v2.miner_tip.borrow().into(),
            ApplyRet::V3(v3) => v3.miner_tip.borrow().into(),
        }
    }

    pub fn penalty(&self) -> TokenAmount {
        match self {
            ApplyRet::V2(v2) => v2.penalty.borrow().into(),
            ApplyRet::V3(v3) => v3.penalty.borrow().into(),
        }
    }

    pub fn msg_receipt(&self) -> Receipt {
        match self {
            ApplyRet::V2(v2) => Receipt::V2(v2.msg_receipt.clone()),
            ApplyRet::V3(v3) => Receipt::V3(v3.msg_receipt.clone()),
        }
    }

    pub fn gas_used(&self) -> u64 {
        match self {
            ApplyRet::V2(v2) => v2.gas_burned as u64,
            ApplyRet::V3(v3) => v3.gas_burned,
        }
    }

    pub fn exec_events(&self) -> Vec<ExecutionEvent_v3> {
        match self {
            ApplyRet::V2(_v2) => todo!(),
            ApplyRet::V3(v3) => v3.exec_trace.clone(),
        }
    }

    pub fn actor_error(&self) -> String {
        match self {
            ApplyRet::V2(_v2) => todo!(),
            ApplyRet::V3(v3) => v3
                .failure_info
                .clone()
                .map_or("".into(), |af| af.to_string()),
        }
    }
}

#[derive(PartialEq, Clone, Debug)]
pub enum Receipt {
    V2(Receipt_v2),
    V3(Receipt_v3),
}

impl Serialize for Receipt {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Receipt::V2(v2) => v2.serialize(serializer),
            Receipt::V3(v3) => v3.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Receipt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Receipt_v2::deserialize(deserializer).map(Receipt::V2)
    }
}

impl Receipt {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Receipt::V2(v2) => ExitCode::new(v2.exit_code.value()),
            Receipt::V3(v3) => v3.exit_code,
        }
    }

    pub fn return_data(&self) -> RawBytes {
        match self {
            Receipt::V2(v2) => RawBytes::from(v2.return_data.to_vec()),
            Receipt::V3(v3) => v3.return_data.clone(),
        }
    }

    pub fn gas_used(&self) -> u64 {
        match self {
            Receipt::V2(v2) => v2.gas_used as u64,
            Receipt::V3(v3) => v3.gas_used,
        }
    }
}

impl From<Receipt_v3> for Receipt {
    fn from(other: Receipt_v3) -> Self {
        Receipt::V3(other)
    }
}

// TODO: use this https://github.com/filecoin-project/lotus/blob/master/chain/types/execresult.go#L35
// to create the equivalent ExecutionTrace structure that we could serialize/deserialize

#[derive(Clone, Debug)]
pub struct TraceGasCharge {
    pub name: Cow<'static, str>,
    pub total_gas: u64,
    pub compute_gas: u64,
    pub other_gas: u64,
    pub duration_nanos: u64,
}

#[derive(Clone, Debug)]
pub struct TraceMessage {
    pub from: Address,
    pub to: Address,
    pub value: TokenAmount,
    pub method_num: MethodNum,
    pub params: Vec<u8>,
    pub codec: u64,
}

#[derive(Clone, Debug)]
pub struct TraceReturn {
    pub exit_code: ExitCode,
    pub return_data: Vec<u8>,
    pub return_codec: u64,
}

#[derive(Clone, Debug)]
pub struct Trace {
    pub msg: TraceMessage,
    pub msg_ret: TraceReturn,
    pub gas_charges: Vec<TraceGasCharge>,
    pub subcalls: Vec<Trace>,
}

//
pub fn build_lotus_trace(
    from: u64,
    to: Address,
    method: u64,
    params: Option<IpldBlock>,
    value: TokenAmount,
    trace_iter: &mut impl Iterator<Item = ExecutionEvent_v3>,
) -> anyhow::Result<Trace> {
    let params = params.unwrap_or_default();
    let mut new_trace = Trace {
        msg: TraceMessage {
            from: Address::new_id(from),
            to,
            value,
            method_num: method,
            params: params.data,
            codec: params.codec,
        },
        msg_ret: TraceReturn {
            exit_code: ExitCode::OK,
            return_data: Vec::new(),
            return_codec: 0,
        },
        gas_charges: vec![],
        subcalls: vec![],
    };

    while let Some(trace) = trace_iter.next() {
        match trace {
            ExecutionEvent_v3::Call {
                from,
                to,
                method,
                params,
                value,
            } => {
                new_trace.subcalls.push(build_lotus_trace(
                    from,
                    to.into(),
                    method,
                    params,
                    value.into(),
                    trace_iter,
                )?);
            }
            ExecutionEvent_v3::CallReturn(exit_code, return_data) => {
                let return_data = return_data.unwrap_or_default();
                new_trace.msg_ret = TraceReturn {
                    exit_code,
                    return_data: return_data.data,
                    return_codec: return_data.codec,
                };
                return Ok(new_trace);
            }
            ExecutionEvent_v3::CallError(syscall_err) => {
                // Errors indicate the message couldn't be dispatched at all
                // (as opposed to failing during execution of the receiving actor).
                // These errors are mapped to exit codes that persist on chain.
                let exit_code = match syscall_err.1 {
                    ErrorNumber::InsufficientFunds => ExitCode::SYS_INSUFFICIENT_FUNDS,
                    ErrorNumber::NotFound => ExitCode::SYS_INVALID_RECEIVER,
                    _ => ExitCode::SYS_ASSERTION_FAILED,
                };

                new_trace.msg_ret = TraceReturn {
                    exit_code,
                    return_data: Default::default(),
                    return_codec: 0,
                };
                return Ok(new_trace);
            }
            ExecutionEvent_v3::GasCharge(GasChargeV3 {
                name,
                compute_gas,
                other_gas,
                elapsed,
            }) => {
                new_trace.gas_charges.push(TraceGasCharge {
                    name,
                    total_gas: (compute_gas + other_gas).round_up(),
                    compute_gas: compute_gas.round_up(),
                    other_gas: other_gas.round_up(),
                    duration_nanos: elapsed
                        .get()
                        .copied()
                        .unwrap_or_default()
                        .as_nanos()
                        .try_into()
                        .unwrap_or(u64::MAX),
                });
            }
            _ => (), // ignore unknown events.
        };
    }

    Err(anyhow!("should have returned on an ExecutionEvent:Return"))
}
