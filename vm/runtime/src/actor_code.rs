use crate::Runtime;
use vm::{InvocOutput, MethodNum, MethodParams};

pub trait ActorCode {
    fn invoke_method(
        &self,
        rt: &dyn Runtime,
        method: MethodNum,
        params: &MethodParams,
    ) -> InvocOutput;
}
