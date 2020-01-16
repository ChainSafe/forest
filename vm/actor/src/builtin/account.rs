// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use address::Address;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{arg_end, ActorCode, Runtime};
use vm::{ExitCode, InvocOutput, MethodNum, MethodParams, SysCode, METHOD_CONSTRUCTOR};

/// AccountActorState includes the address for the actor
pub struct AccountActorState {
    pub address: Address,
}

#[derive(FromPrimitive)]
pub enum AccountMethod {
    Constructor = METHOD_CONSTRUCTOR,
}

impl AccountMethod {
    /// from_method_num converts a method number into an AccountMethod enum
    fn from_method_num(m: MethodNum) -> Option<AccountMethod> {
        FromPrimitive::from_i32(m.into())
    }
}

#[derive(Clone)]
pub struct AccountActorCode;

impl AccountActorCode {
    /// Constructor for Account actor
    fn constructor<RT: Runtime>(rt: &RT) -> InvocOutput {
        // Intentionally left blank
        rt.success_return()
    }
}

impl ActorCode for AccountActorCode {
    fn invoke_method<RT: Runtime>(
        &self,
        rt: &RT,
        method: MethodNum,
        params: &MethodParams,
    ) -> InvocOutput {
        match AccountMethod::from_method_num(method) {
            Some(AccountMethod::Constructor) => {
                // Assert no parameters passed
                arg_end(params, rt);
                Self::constructor(rt)
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
