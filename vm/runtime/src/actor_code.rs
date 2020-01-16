// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::Runtime;
use vm::{InvocOutput, MethodNum, MethodParams, Serialized};

pub trait ActorCode {
    /// Invokes method with runtime on the actor's code
    fn invoke_method<RT: Runtime>(
        &self,
        rt: &RT,
        method: MethodNum,
        params: &MethodParams,
    ) -> InvocOutput;
}

pub fn check_args<RT: Runtime>(_params: &MethodParams, rt: &RT, cond: bool) {
    if !cond {
        rt.abort_arg();
    }
    // TODO assume params validation on finished spec
}

pub fn arg_pop<RT: Runtime>(params: &mut MethodParams, rt: &RT) -> Serialized {
    check_args(params, rt, !params.is_empty());
    params.remove(0)
}

pub fn arg_end<RT: Runtime>(params: &MethodParams, rt: &RT) {
    check_args(params, rt, params.is_empty())
}
