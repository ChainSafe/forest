// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::State;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
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
    pub fn constructor<RT: Runtime>(_rt: &RT, _params: ConstructorParams) {
        // TODO
        todo!()
        // Acquire state
        // Update actor substate
    }

    /// Exec init actor
    pub fn exec<RT: Runtime>(_rt: &RT, _params: &Serialized) {
        // TODO update and include exec params type and return
        todo!()
    }
}

impl ActorCode for Actor {
    fn invoke_method<RT: Runtime>(&self, rt: &RT, method: MethodNum, params: &Serialized) {
        // Create mutable copy of params for usage in functions
        let params: &mut Serialized = &mut params.clone();
        match Method::from_method_num(method) {
            Some(Method::Constructor) => {
                // TODO unfinished spec

                Self::constructor(
                    rt,
                    ConstructorParams {
                        network_name: "".into(),
                    },
                )
            }
            Some(Method::Exec) => {
                // TODO deserialize CodeID on finished spec
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
