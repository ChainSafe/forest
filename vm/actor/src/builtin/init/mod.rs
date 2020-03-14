// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::State;
use address::Address;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{CodeID, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR, METHOD_PLACEHOLDER};

/// Init actor methods available
#[derive(FromPrimitive)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    Exec = METHOD_PLACEHOLDER,
    GetActorIDForAddress = METHOD_PLACEHOLDER + 1,
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
    fn constructor<RT: Runtime>(_rt: &RT, _params: ConstructorParams) {
        // Acquire state
        // Update actor substate
    }
    fn exec<RT: Runtime>(_rt: &RT, _code: CodeID, _params: &Serialized) {
        todo!()
    }
    fn get_actor_id_for_address<RT: Runtime>(_rt: &RT, _address: Address) {
        // TODO
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
                Self::exec(rt, CodeID::Init, params)
            }
            Some(Method::GetActorIDForAddress) => {
                // Unmarshall address parameter
                // TODO unfinished spec

                // Errors checked, get actor by address
                Self::get_actor_id_for_address(rt, Address::default())
            }
            _ => {
                // Method number does not match available, abort in runtime
                rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method".to_owned());
                unreachable!();
            }
        }
    }
}
