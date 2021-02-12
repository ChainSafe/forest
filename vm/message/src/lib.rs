// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// workaround for a compiler bug, see https://github.com/rust-lang/rust/issues/55779
extern crate serde;

pub mod chain_message;
pub mod message_receipt;
pub mod signed_message;
pub mod unsigned_message;

pub use chain_message::ChainMessage;
pub use message_receipt::MessageReceipt;
pub use signed_message::SignedMessage;
pub use unsigned_message::UnsignedMessage;

use address::Address;
use vm::{MethodNum, Serialized, TokenAmount};

/// Message interface to interact with Signed and unsigned messages in a generic context.
pub trait Message {
    /// Returns the from address of the message.
    fn from(&self) -> &Address;
    /// Returns the destination address of the message.
    fn to(&self) -> &Address;
    /// Returns the message sequence or nonce.
    fn sequence(&self) -> u64;
    /// Returns the amount sent in message.
    fn value(&self) -> &TokenAmount;
    /// Returns the method number to be called.
    fn method_num(&self) -> MethodNum;
    /// Returns the encoded parameters for the method call.
    fn params(&self) -> &Serialized;
    /// sets the gas limit for the message.
    fn set_gas_limit(&mut self, amount: i64);
    /// sets a new sequence to the message.
    fn set_sequence(&mut self, sequence: u64);
    /// Returns the gas limit for the message.
    fn gas_limit(&self) -> i64;
    /// Returns the required funds for the message.
    fn required_funds(&self) -> TokenAmount;
    /// gets gas fee cap for the message.
    fn gas_fee_cap(&self) -> &TokenAmount;
    /// gets gas premium for the message.
    fn gas_premium(&self) -> &TokenAmount;
    /// sets the gas fee cap.
    fn set_gas_fee_cap(&mut self, cap: TokenAmount);
    /// sets the gas premium.
    fn set_gas_premium(&mut self, prem: TokenAmount);
}
