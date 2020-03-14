// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::InitActorState;
use address::Address;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{CodeID, ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR, METHOD_PLACEHOLDER};

#[derive(FromPrimitive)]
pub enum InitMethod {
    Constructor = METHOD_CONSTRUCTOR,
    Exec = METHOD_PLACEHOLDER,
    GetActorIDForAddress = METHOD_PLACEHOLDER + 1,
}

impl InitMethod {
    /// from_method_num converts a method number into an InitMethod enum
    fn from_method_num(m: MethodNum) -> Option<InitMethod> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

pub struct InitActor;
impl InitActor {
    fn constructor<RT: Runtime>(_rt: &RT) {
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

impl ActorCode for InitActor {
    fn invoke_method<RT: Runtime>(&self, rt: &RT, method: MethodNum, params: &Serialized) {
        // Create mutable copy of params for usage in functions
        let params: &mut Serialized = &mut params.clone();
        match InitMethod::from_method_num(method) {
            Some(InitMethod::Constructor) => {
                // TODO unfinished spec

                Self::constructor(rt)
            }
            Some(InitMethod::Exec) => {
                // TODO deserialize CodeID on finished spec
                Self::exec(rt, CodeID::Init, params)
            }
            Some(InitMethod::GetActorIDForAddress) => {
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

#[cfg(test)]
mod test {

    #[test]
    fn assign_id() {
        // TODO replace with new functionality test on full impl
    }
}
