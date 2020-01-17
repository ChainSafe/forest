// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::Runtime;
use vm::{InvocOutput, MethodNum, MethodParams, Serialized};

/// Interface for invoking methods on an Actor
pub trait ActorCode {
    /// Invokes method with runtime on the actor's code. Method number will match one
    /// defined by the Actor, and parameters will be serialized and used in execution
    fn invoke_method<RT: Runtime>(
        &self,
        rt: &RT,
        method: MethodNum,
        params: &MethodParams,
    ) -> InvocOutput;
}

/// This function will verify the parameters of a method invocation
pub fn check_args<RT: Runtime>(_params: &MethodParams, rt: &RT, cond: bool) {
    if !cond {
        rt.abort_arg();
    }
    // TODO assume there will be params validation on finished spec
}

/// Will return the next serialized parameter from the parameters and abort if empty
pub fn arg_pop<RT: Runtime>(params: &mut MethodParams, rt: &RT) -> Serialized {
    check_args(params, rt, !params.is_empty());
    params.remove(0)
}

/// Function will assert that there were no other parameters provided, and abort if so
pub fn arg_end<RT: Runtime>(params: &MethodParams, rt: &RT) {
    check_args(params, rt, params.is_empty())
}
