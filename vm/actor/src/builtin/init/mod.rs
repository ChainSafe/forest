// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::State;
use crate::empty_return;
use ipld_blockstore::BlockStore;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::{ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

/// Init actor methods available
#[derive(FromPrimitive)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    Exec = 2,
}

impl Method {
    /// from_method_num converts a method number into an Method enum
    fn from_method_num(m: MethodNum) -> Option<Method> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

/// Constructor parameters
pub struct ConstructorParams {
    pub network_name: String,
}

/// Init actor
pub struct Actor;
impl Actor {
    /// Init actor constructor
    pub fn constructor<BS, RT>(_rt: &RT, _params: ConstructorParams)
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO
        todo!()
    }

    /// Exec init actor
    pub fn exec<BS, RT>(_rt: &RT, _params: &Serialized) -> Serialized
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO update and include exec params type and return
        todo!()
    }
}

impl ActorCode for Actor {
    fn invoke_method<BS, RT>(&self, rt: &RT, method: MethodNum, params: &Serialized) -> Serialized
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match Method::from_method_num(method) {
            Some(Method::Constructor) => {
                Self::constructor(rt, params.deserialize().unwrap());
                empty_return()
            }
            Some(Method::Exec) => {
                // TODO update to correct param type
                Self::exec(rt, params)
            }
            _ => {
                // Method number does not match available, abort in runtime
                rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method".to_owned());
                unreachable!();
            }
        }
    }
}

impl Serialize for ConstructorParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [&self.network_name].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ConstructorParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [network_name]: [String; 1] = Deserialize::deserialize(deserializer)?;
        Ok(Self { network_name })
    }
}
