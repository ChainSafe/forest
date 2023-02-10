// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod chain_message;
pub mod message;
pub mod signed_message;

use std::rc::Rc;

pub use chain_message::ChainMessage;
use forest_shim::{
    address::{Address, AddressRef},
    econ::TokenAmount,
};
use fvm_ipld_encoding::RawBytes;
use fvm_shared::MethodNum;
pub use signed_message::SignedMessage;

/// Message interface to interact with Signed and unsigned messages in a generic
/// context.
pub trait Message {
    /// Returns the from address of the message.
    fn from(&self) -> AddressRef;
    /// Returns the destination address of the message.
    fn to(&self) -> AddressRef;
    /// Returns the message sequence or nonce.
    fn sequence(&self) -> u64;
    /// Returns the amount sent in message.
    fn value(&self) -> Rc<TokenAmount>;
    /// Returns the method number to be called.
    fn method_num(&self) -> MethodNum;
    /// Returns the encoded parameters for the method call.
    fn params(&self) -> Rc<RawBytes>;
    /// sets the gas limit for the message.
    fn set_gas_limit(&mut self, amount: u64);
    /// sets a new sequence to the message.
    fn set_sequence(&mut self, sequence: u64);
    /// Returns the gas limit for the message.
    fn gas_limit(&self) -> u64;
    /// Returns the required funds for the message.
    fn required_funds(&self) -> TokenAmount;
    /// gets gas fee cap for the message.
    fn gas_fee_cap(&self) -> Rc<TokenAmount>;
    /// gets gas premium for the message.
    fn gas_premium(&self) -> Rc<TokenAmount>;
    /// sets the gas fee cap.
    fn set_gas_fee_cap(&mut self, cap: TokenAmount);
    /// sets the gas premium.
    fn set_gas_premium(&mut self, prem: TokenAmount);
}
