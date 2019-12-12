use address::Address;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
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
        FromPrimitive::from_i32(m.0)
    }
}

#[derive(Clone)]
pub struct AccountActorCode;

impl AccountActorCode {
    /// Constructor for Account actor
    pub(crate) fn constructor(rt: &dyn Runtime) -> InvocOutput {
        // Intentionally left blank
        rt.success_return()
    }
}

impl ActorCode for AccountActorCode {
    fn invoke_method(
        &self,
        rt: &dyn Runtime,
        method: MethodNum,
        params: &MethodParams,
    ) -> InvocOutput {
        match AccountMethod::from_method_num(method) {
            Some(AccountMethod::Constructor) => {
                rt.assert(params.0.is_empty());
                AccountActorCode::constructor(rt)
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
