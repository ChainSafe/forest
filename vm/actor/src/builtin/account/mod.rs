// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;

pub use self::state::AccountActorState;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{ExitCode, MethodNum, Serialized, METHOD_CONSTRUCTOR};

#[derive(FromPrimitive)]
pub enum AccountMethod {
    Constructor = METHOD_CONSTRUCTOR,
}

impl AccountMethod {
    /// from_method_num converts a method number into an AccountMethod enum
    fn from_method_num(m: MethodNum) -> Option<AccountMethod> {
        FromPrimitive::from_u64(u64::from(m))
    }
}

pub struct AccountActor;
impl AccountActor {
    /// Constructor for Account actor
    fn constructor<RT: Runtime>(_rt: &RT) {
        // Intentionally left blank
    }
}

impl ActorCode for AccountActor {
    fn invoke_method<RT: Runtime>(&self, rt: &RT, method: MethodNum, _params: &Serialized) {
        match AccountMethod::from_method_num(method) {
            Some(AccountMethod::Constructor) => {
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
