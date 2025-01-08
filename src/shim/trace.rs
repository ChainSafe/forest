// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use self::ExecutionEvent as EShim;
use crate::shim::{
    address::Address as ShimAddress, econ::TokenAmount as ShimTokenAmount,
    error::ExitCode as ShimExitCode, gas::GasCharge as ShimGasCharge,
    kernel::SyscallError as ShimSyscallError, state_tree::ActorID as ShimActorId,
    state_tree::ActorState as ShimActorState,
};
use cid::Cid;
use fvm2::trace::ExecutionEvent as E2;
use fvm3::trace::ExecutionEvent as E3;
use fvm4::trace::ExecutionEvent as E4;
use fvm_ipld_encoding::{ipld_block::IpldBlock, RawBytes};
use itertools::Either;

#[derive(Debug, Clone)]
pub enum ExecutionEvent {
    GasCharge(ShimGasCharge),
    Call(Call),
    CallReturn(CallReturn),
    CallAbort(ShimExitCode),
    CallError(ShimSyscallError),
    Log(String),
    InvokeActor(Either<Cid, InvokeActor>),
    Unknown(Either<E3, Either<E4, E2>>),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CallReturn {
    pub exit_code: Option<ShimExitCode>,
    pub data: Either<RawBytes, Option<IpldBlock>>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Call {
    /// `ActorID`
    pub from: u64,
    pub to: ShimAddress,
    pub method_num: u64,
    pub params: Either<RawBytes, Option<IpldBlock>>,
    pub value: ShimTokenAmount,
    pub gas_limit: Option<u64>,
    pub read_only: Option<bool>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InvokeActor {
    pub id: ShimActorId,
    pub state: ShimActorState,
}

impl From<E2> for ExecutionEvent {
    fn from(value: E2) -> Self {
        match value {
            E2::GasCharge(gc) => EShim::GasCharge(gc.into()),
            E2::Call {
                from,
                to,
                method,
                params,
                value,
            } => EShim::Call(Call {
                from,
                to: to.into(),
                method_num: method,
                params: Either::Left(params),
                value: value.into(),
                gas_limit: None,
                read_only: None,
            }),
            E2::CallReturn(data) => EShim::CallReturn(CallReturn {
                exit_code: None,
                data: Either::Left(data),
            }),
            E2::CallAbort(ab) => EShim::CallAbort(ab.into()),
            E2::CallError(err) => EShim::CallError(err.into()),
            E2::Log(s) => EShim::Log(s),
            e => EShim::Unknown(Either::Right(Either::Right(e))),
        }
    }
}

impl From<E3> for ExecutionEvent {
    fn from(value: E3) -> Self {
        match value {
            E3::GasCharge(gc) => EShim::GasCharge(gc.into()),
            E3::Call {
                from,
                to,
                method,
                params,
                value,
                gas_limit,
                read_only,
            } => EShim::Call(Call {
                from,
                to: to.into(),
                method_num: method,
                params: Either::Right(params),
                value: value.into(),
                gas_limit: Some(gas_limit),
                read_only: Some(read_only),
            }),
            E3::CallReturn(exit_code, data) => EShim::CallReturn(CallReturn {
                exit_code: Some(exit_code.into()),
                data: Either::Right(data),
            }),
            E3::CallError(err) => EShim::CallError(err.into()),
            E3::InvokeActor { id, state } => EShim::InvokeActor(Either::Right(InvokeActor {
                id,
                state: state.into(),
            })),
            e => EShim::Unknown(Either::Left(e)),
        }
    }
}

impl From<E4> for ExecutionEvent {
    fn from(value: E4) -> Self {
        match value {
            E4::GasCharge(gc) => EShim::GasCharge(gc.into()),
            E4::Call {
                from,
                to,
                method,
                params,
                value,
                gas_limit,
                read_only,
            } => EShim::Call(Call {
                from,
                to: to.into(),
                method_num: method,
                params: Either::Right(params),
                value: value.into(),
                gas_limit: Some(gas_limit),
                read_only: Some(read_only),
            }),
            E4::CallReturn(exit_code, data) => EShim::CallReturn(CallReturn {
                exit_code: Some(exit_code.into()),
                data: Either::Right(data),
            }),
            E4::CallError(err) => EShim::CallError(err.into()),
            E4::InvokeActor { id, state } => EShim::InvokeActor(Either::Right(InvokeActor {
                id,
                state: state.into(),
            })),
            e => EShim::Unknown(Either::Right(Either::Left(e))),
        }
    }
}
