// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

pub use self::state::{LaneState, Merge, State};
pub use self::types::*;
// use crate::{assert_empty_params, empty_return, SYSTEM_ACTOR_ADDR};
// use address::Address;
use ipld_blockstore::BlockStore;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
// use serde::{Deserialize, Deserializer, Serialize, Serializer};
use clock::ChainEpoch;
use vm::{ActorError, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

/// Payment Channel actor methods available
#[derive(FromPrimitive)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    UpdateChannelState = 2,
    Settle = 3,
    Collect = 4,
}

impl Method {
    /// Converts a method number into an Method enum
    fn from_method_num(m: MethodNum) -> Option<Method> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

/// Payment Channel actor
pub struct Actor;
impl Actor {}

impl ActorCode for Actor {
    fn invoke_method<BS, RT>(
        &self,
        rt: &RT,
        method: MethodNum,
        _params: &Serialized,
    ) -> Result<Serialized, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match Method::from_method_num(method) {
            // Some(Method::Constructor) => {
            //     Self::constructor(rt, params.deserialize().unwrap())?;
            //     Ok(empty_return())
            // }
            // Some(Method::EpochTick) => {
            //     assert_empty_params(params);
            //     Self::epoch_tick(rt)?;
            //     Ok(empty_return())
            // }
            _ => Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method")),
        }
    }
}
