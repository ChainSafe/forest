// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::{ActorID, CodeID};
use vm::{
    ExitCode, InvocOutput, MethodNum, MethodParams, SysCode, METHOD_CONSTRUCTOR, METHOD_PLACEHOLDER,
};

use address::Address;
use encoding::Cbor;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{arg_end, arg_pop, check_args, ActorCode, Runtime};
use std::collections::HashMap;

/// InitActorState is reponsible for creating
#[derive(Default)]
pub struct InitActorState {
    // TODO possibly switch this to a hamt to be able to dump the data and save as Cid
    _address_map: HashMap<Address, ActorID>,
    next_id: ActorID,
}

impl InitActorState {
    /// Assigns next available ID and incremenets the next_id value from state
    pub fn assign_next_id(&mut self) -> ActorID {
        let next = self.next_id;
        self.next_id.0 += 1;
        next
    }
}

#[derive(FromPrimitive)]
pub enum InitMethod {
    Constructor = METHOD_CONSTRUCTOR,
    Exec = METHOD_PLACEHOLDER,
    GetActorIDForAddress = METHOD_PLACEHOLDER + 1,
}

impl InitMethod {
    /// from_method_num converts a method number into an InitMethod enum
    fn from_method_num(m: MethodNum) -> Option<InitMethod> {
        FromPrimitive::from_i32(m.into())
    }
}

pub struct InitActorCode;
impl InitActorCode {
    fn constructor(rt: &dyn Runtime) -> InvocOutput {
        // Acquire state
        // Update actor substate

        rt.success_return()
    }
    fn exec(rt: &dyn Runtime, _code: CodeID, _params: &MethodParams) -> InvocOutput {
        // TODO
        let addr = Address::new_id(0).unwrap();
        rt.value_return(addr.marshal_cbor().unwrap())
    }
    fn get_actor_id_for_address(rt: &dyn Runtime, _address: Address) -> InvocOutput {
        // TODO
        rt.value_return(ActorID(0).marshal_cbor().unwrap())
    }
}

impl ActorCode for InitActorCode {
    fn invoke_method(
        &self,
        rt: &dyn Runtime,
        method: MethodNum,
        params_in: &MethodParams,
    ) -> InvocOutput {
        // Create mutable copy of params for usage in functions
        let params: &mut MethodParams = &mut params_in.clone();
        match InitMethod::from_method_num(method) {
            Some(InitMethod::Constructor) => {
                // validate no arguments passed in
                arg_end(params, rt);

                Self::constructor(rt)
            }
            Some(InitMethod::Exec) => {
                // TODO deserialize CodeID on finished spec
                let _ = arg_pop(params, rt);
                check_args(params, rt, true);
                Self::exec(rt, CodeID::Init, params)
            }
            Some(InitMethod::GetActorIDForAddress) => {
                // Pop and unmarshall address parameter
                let addr_res = Address::unmarshal_cbor(&arg_pop(params, rt).bytes());

                // validate addr deserialization and parameters
                check_args(params, rt, addr_res.is_ok());
                arg_end(params, rt);

                // Errors checked, get actor by address
                Self::get_actor_id_for_address(rt, addr_res.unwrap())
            }
            _ => {
                // Method number does not match available, abort in runtime
                rt.abort(
                    ExitCode::SystemErrorCode(SysCode::InvalidMethod),
                    "Invalid method",
                );
                unreachable!();
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn assign_id() {
        let mut actor_s = InitActorState::default();
        assert_eq!(actor_s.assign_next_id().0, 0);
        assert_eq!(actor_s.assign_next_id().0, 1);
        assert_eq!(actor_s.assign_next_id().0, 2);
    }
}
