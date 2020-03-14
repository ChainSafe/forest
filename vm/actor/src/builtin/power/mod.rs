// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::PowerActorState;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

/// Method definitions for Storage Power Actor
#[derive(FromPrimitive)]
pub enum PowerMethod {
    /// Constructor for Storage Power Actor
    Constructor = METHOD_CONSTRUCTOR,
    // TODO add other methods on finished spec
    /// Gets the total storage for the network
    GetTotalStorage = 5,
}

impl PowerMethod {
    /// from_method_num converts a method number into an PowerMethod enum
    fn from_method_num(m: MethodNum) -> Option<PowerMethod> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

#[derive(Clone)]
pub struct PowerActor;
impl PowerActor {
    /// Constructor for StoragePower actor
    fn constructor<RT: Runtime>(_rt: &RT) {
        // TODO
        todo!();
    }
    /// Withdraw available funds from StoragePower map
    fn get_total_storage<RT: Runtime>(_rt: &RT) {
        // TODO
        todo!()
    }
}

impl ActorCode for PowerActor {
    fn invoke_method<RT: Runtime>(&self, rt: &RT, method: MethodNum, _params: &Serialized) {
        match PowerMethod::from_method_num(method) {
            // TODO determine parameters for each method on finished spec
            Some(PowerMethod::Constructor) => Self::constructor(rt),
            Some(PowerMethod::GetTotalStorage) => Self::get_total_storage(rt),
            _ => {
                rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method".to_owned());
                unreachable!();
            }
        }
    }
}
