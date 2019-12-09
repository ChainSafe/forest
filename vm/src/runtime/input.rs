use crate::actor::{MethodNum, MethodParams};
use crate::TokenAmount;

use address::Address;

/// Input variables for actor method invocation.
pub struct InvocInput {
    pub to: Address,
    pub method: MethodNum,
    pub params: MethodParams,
    pub value: TokenAmount,
}
