// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use crate::check_empty_params;
use address::Address;
use encoding::{tuple::*, Cbor};
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser;
use runtime::{ActorCode, Runtime};
use serde::{Deserialize, Serialize};
use vm::{ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR};

#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    SendMethod = 2,
    SendMarshalCBORFailure = 3,
    ReturnMarshalCBORFailure = 4,
    RuntimeTransactionMarshalCBORFailure = 5,
}

#[derive(Serialize, Deserialize)]
pub struct SendParams {
    pub to: Address,
    #[serde(with = "bigint_ser")]
    pub value: TokenAmount,
    pub method: u64,
    pub params: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct SendReturn {
    pub return_bytes: Serialized,
    pub code: ExitCode,
}

#[derive(Default, Serialize, Deserialize)]
pub struct FailToMarshalCBOR {}

impl Cbor for FailToMarshalCBOR {}

#[derive(Default, Serialize_tuple, Deserialize_tuple)]
pub struct State {
    opt_fail: Vec<Option<FailToMarshalCBOR>>,
}

impl Cbor for State {}

pub struct Actor;

impl Actor {
    fn constructor<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;

        rt.create(&State::default())?;
        Ok(())
    }

    fn send<BS, RT>(rt: &mut RT, params: SendParams) -> Result<SendReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;

        let res = rt.send(
            params.to,
            params.method,
            Serialized::serialize(params.params).unwrap(),
            params.value,
        );

        let return_bytes = res.map_err(|_| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                "failed to get return bytes".to_string(),
            )
        })?;

        Ok(SendReturn {
            return_bytes,
            code: ExitCode::Ok,
        })
    }

    fn send_failure<BS, RT>(rt: &mut RT, params: SendParams) -> Result<SendReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;

        let res = rt.send(
            params.to,
            params.method,
            Serialized::serialize(FailToMarshalCBOR::default()).unwrap(),
            params.value,
        );

        let return_bytes = res.map_err(|_| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                "failed to get return bytes".to_string(),
            )
        })?;

        Ok(SendReturn {
            return_bytes,
            code: ExitCode::Ok,
        })
    }

    fn return_failure<BS, RT>(rt: &mut RT) -> Result<FailToMarshalCBOR, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        Ok(FailToMarshalCBOR::default())
    }

    fn runtime_transaction_failure<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;

        rt.transaction(|st: &mut State, _| {
            st.opt_fail = vec![];
        })?;

        Ok(())
    }
}

impl ActorCode for Actor {
    /// Invokes method with runtime on the actor's code. Method number will match one
    /// defined by the Actor, and parameters will be serialized and used in execution
    fn invoke_method<BS, RT>(
        &self,
        rt: &mut RT,
        method: MethodNum,
        params: &Serialized,
    ) -> Result<Serialized, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match FromPrimitive::from_u64(method) {
            Some(Method::Constructor) => {
                check_empty_params(params)?;
                Self::constructor(rt)?;
                Ok(Serialized::default())
            }
            Some(Method::SendMethod) => {
                let res = Self::send(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::SendMarshalCBORFailure) => {
                let res = Self::send_failure(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::ReturnMarshalCBORFailure) => {
                check_empty_params(params)?;
                let res = Self::return_failure(rt)?;
                Ok(Serialized::serialize(res)?)
            }

            Some(Method::RuntimeTransactionMarshalCBORFailure) => {
                check_empty_params(params)?;
                let _ = Self::runtime_transaction_failure(rt)?;
                Ok(Serialized::default())
            }

            _ => Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method")),
        }
    }
}
