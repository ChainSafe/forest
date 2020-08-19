// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::check_empty_params;
use address::Address;
use encoding::{tuple::*, Cbor};
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use serde::de::{self, Deserializer};
use serde::ser::{self, Serializer};
use serde::{Deserialize, Serialize};
use vm::{ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR};

// * Updated to specs-actors commit: e3ae346e69f7ad353b4eab6c20d8c6a5f497a039

#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    Send = 2,
    SendMarshalCBORFailure = 3,
    ReturnMarshalCBORFailure = 4,
    RuntimeTransactionMarshalCBORFailure = 5,
}

#[derive(Serialize, Deserialize)]
pub struct SendParams {
    pub to: Address,
    #[serde(with = "bigint_ser")]
    pub value: TokenAmount,
    pub method: MethodNum,
    pub params: Serialized,
}

#[derive(Serialize, Deserialize)]
pub struct SendReturn {
    pub return_bytes: Option<Serialized>,
    pub code: ExitCode,
}

#[derive(Default)]
pub struct FailToMarshalCBOR {}

impl Serialize for FailToMarshalCBOR {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Err(ser::Error::custom(
            "Automatic fail when serializing FailToMarshalCBOR",
        ))
    }
}

impl<'de> Deserialize<'de> for FailToMarshalCBOR {
    fn deserialize<D>(_deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        Err(de::Error::custom(
            "Automatic fail when deserializing FailToMarshalCBOR",
        ))
    }
}

impl Cbor for FailToMarshalCBOR {}

#[derive(Default, Serialize_tuple, Deserialize_tuple)]
pub struct State {
    // OptFailToMarshalCBOR is to be used as an Option<T>, with T
    // specialized to FailToMarshalCBOR. If the slice contains no values, the
    // State struct will serialize/deserialize without issue. If the slice contains
    // more than zero values, serialization/deserialization will fail.
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

        let res = rt.send(params.to, params.method, params.params, params.value);

        match res {
            Ok(return_bytes) => Ok(SendReturn {
                return_bytes: Some(return_bytes),
                code: ExitCode::Ok,
            }),
            Err(e) => Ok(SendReturn {
                return_bytes: None,
                code: e.exit_code(),
            }),
        }
    }

    fn send_marshal_cbor_failure<BS, RT>(
        rt: &mut RT,
        params: SendParams,
    ) -> Result<SendReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;

        let res = rt.send(
            params.to,
            params.method,
            Serialized::serialize(FailToMarshalCBOR::default())?,
            params.value,
        );

        match res {
            Ok(return_bytes) => Ok(SendReturn {
                return_bytes: Some(return_bytes),
                code: ExitCode::Ok,
            }),
            Err(e) => Ok(SendReturn {
                return_bytes: None,
                code: e.exit_code(),
            }),
        }
    }

    fn return_marshal_cbor_failure<BS, RT>(rt: &mut RT) -> Result<FailToMarshalCBOR, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        Ok(FailToMarshalCBOR::default())
    }

    fn runtime_transaction_marshal_cbor_failure<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
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
            Some(Method::Send) => {
                let res = Self::send(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::SendMarshalCBORFailure) => {
                let res = Self::send_marshal_cbor_failure(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::ReturnMarshalCBORFailure) => {
                check_empty_params(params)?;
                let res = Self::return_marshal_cbor_failure(rt)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::RuntimeTransactionMarshalCBORFailure) => {
                check_empty_params(params)?;
                Self::runtime_transaction_marshal_cbor_failure(rt)?;
                Ok(Serialized::default())
            }
            None => Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method")),
        }
    }
}
