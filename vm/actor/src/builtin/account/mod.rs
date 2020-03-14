// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::State;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

/// Account actor methods available
#[derive(FromPrimitive)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
}

impl Method {
    /// from_method_num converts a method number into a Method enum
    fn from_method_num(m: MethodNum) -> Option<Method> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

/// Account Actor
pub struct Actor;
impl Actor {
    /// Constructor for Account actor
    fn constructor<RT: Runtime>(_rt: &RT) {
        // Intentionally left blank
    }
}

impl ActorCode for Actor {
    fn invoke_method<RT: Runtime>(&self, rt: &RT, method: MethodNum, _params: &Serialized) {
        match Method::from_method_num(method) {
            Some(Method::Constructor) => {
                // TODO unfinished spec
                Self::constructor(rt)
            }
            _ => {
                rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method".to_owned());
                unreachable!();
            }
        }
    }
}
