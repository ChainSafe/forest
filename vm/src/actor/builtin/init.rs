use crate::actor::{
    ActorCode, ActorID, CodeID, MethodNum, MethodParams, METHOD_CONSTRUCTOR, METHOD_PLACEHOLDER,
};
use crate::runtime::{InvocOutput, Runtime};
use crate::{ExitCode, SysCode};

use address::Address;
use encoding::Cbor;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
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
        FromPrimitive::from_i32(m.0)
    }
}

pub struct InitActorCode;
impl InitActorCode {
    pub(crate) fn constructor(rt: &dyn Runtime) -> InvocOutput {
        // Acquire state
        // Update actor substate

        rt.success_return()
    }
    pub(crate) fn exec(_r: &dyn Runtime, _code: CodeID, _params: &MethodParams) -> Address {
        // TODO
        Address::new_id(0).unwrap()
    }
    pub(crate) fn get_actor_id_for_address(_r: &dyn Runtime, _address: Address) -> ActorID {
        // TODO
        ActorID(0)
    }
}

impl ActorCode for InitActorCode {
    fn invoke_method(
        &self,
        rt: &dyn Runtime,
        method: MethodNum,
        params: &MethodParams,
    ) -> InvocOutput {
        match InitMethod::from_method_num(method) {
            Some(InitMethod::Constructor) => InitActorCode::constructor(rt),
            Some(InitMethod::Exec) => {
                // TODO get codeID from params
                let addr = InitActorCode::exec(rt, CodeID::Init, params);
                rt.value_return(addr.marshal_cbor().unwrap())
            }
            Some(InitMethod::GetActorIDForAddress) => {
                // TODO get address from params
                let actor =
                    InitActorCode::get_actor_id_for_address(rt, Address::new_id(1).unwrap());
                rt.value_return(actor.marshal_cbor().unwrap())
            }
            _ => {
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
