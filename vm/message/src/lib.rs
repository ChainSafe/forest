// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod chain_message;
pub mod message;
pub mod message_receipt;
pub mod signed_message;

pub use chain_message::ChainMessage;
use fvm_ipld_encoding::RawBytes;
pub use message_receipt::MessageReceipt;
pub use signed_message::SignedMessage;

use fvm_shared::address::Address;
use fvm_shared::bigint::BigInt;
use fvm_shared::MethodNum;

/// Message interface to interact with Signed and unsigned messages in a generic context.
pub trait Message {
    /// Returns the from address of the message.
    fn from(&self) -> &Address;
    /// Returns the destination address of the message.
    fn to(&self) -> &Address;
    /// Returns the message sequence or nonce.
    fn sequence(&self) -> u64;
    /// Returns the amount sent in message.
    fn value(&self) -> &BigInt;
    /// Returns the method number to be called.
    fn method_num(&self) -> MethodNum;
    /// Returns the encoded parameters for the method call.
    fn params(&self) -> &RawBytes;
    /// sets the gas limit for the message.
    fn set_gas_limit(&mut self, amount: i64);
    /// sets a new sequence to the message.
    fn set_sequence(&mut self, sequence: u64);
    /// Returns the gas limit for the message.
    fn gas_limit(&self) -> i64;
    /// Returns the required funds for the message.
    fn required_funds(&self) -> BigInt;
    /// gets gas fee cap for the message.
    fn gas_fee_cap(&self) -> &BigInt;
    /// gets gas premium for the message.
    fn gas_premium(&self) -> &BigInt;
    /// sets the gas fee cap.
    fn set_gas_fee_cap(&mut self, cap: BigInt);
    /// sets the gas premium.
    fn set_gas_premium(&mut self, prem: BigInt);
}
