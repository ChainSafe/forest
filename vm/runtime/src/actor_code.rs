use crate::Runtime;
use vm::{InvocOutput, MethodNum, MethodParams, Serialized};

pub trait ActorCode {
    /// Invokes method with runtime on the actor's code
    fn invoke_method(
        &self,
        rt: &dyn Runtime,
        method: MethodNum,
        params: &MethodParams,
    ) -> InvocOutput;
}

pub fn check_args(_params: &MethodParams, rt: &dyn Runtime, cond: bool) {
    if !cond {
        rt.abort_arg();
    }
    // TODO assume params validation on finished spec
}

pub fn arg_pop(params: &mut MethodParams, rt: &dyn Runtime) -> Serialized {
    check_args(params, rt, !params.is_empty());
    params.remove(0)
}

pub fn arg_end(params: &MethodParams, rt: &dyn Runtime) {
    check_args(params, rt, params.is_empty())
}
