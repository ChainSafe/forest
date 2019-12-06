use crate::actor::MethodParams;
use crate::TokenAmount;

use address::Address;

/// Input variables for actor method invocation.
pub struct InvocInput {
    pub to: Address,
    pub method: i32,          // TODO define method number type
    pub params: MethodParams, // TODO define method params
    pub value: TokenAmount,
}
