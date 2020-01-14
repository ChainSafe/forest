// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

mod message_receipt;
mod signed_message;
mod unsigned_message;

pub use message_receipt::*;
pub use signed_message::*;
pub use unsigned_message::*;

use address::Address;
use vm::{MethodNum, MethodParams, TokenAmount};

pub trait Message {
    /// from returns the from address of the message
    fn from(&self) -> Address;
    /// to returns the destination address of the message
    fn to(&self) -> Address;
    /// sequence returns the message sequence or nonce
    fn sequence(&self) -> u64;
    /// value returns the amount sent in message
    fn value(&self) -> TokenAmount;
    /// method_num returns the method number to be called
    fn method_num(&self) -> MethodNum;
    /// params returns the encoded parameters for the method call
    fn params(&self) -> MethodParams;
    /// gas_price returns gas price for the message
    // TODO: change u128 to BigUint if needed in future
    fn gas_price(&self) -> u128;
    /// gas_limit returns the gas limit for the message
    fn gas_limit(&self) -> u128;
}
