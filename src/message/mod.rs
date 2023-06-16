// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod chain_message;
pub mod message;
pub mod signed_message;

pub use chain_message::ChainMessage;
use forest_shim::{address::Address, econ::TokenAmount, message::Message as ShimMessage};
use fvm_ipld_encoding3::RawBytes;
use fvm_shared::MethodNum;
pub use signed_message::SignedMessage;

/// Message interface to interact with Signed and unsigned messages in a generic
/// context.
pub trait Message {
    /// Returns the from address of the message.
    fn from(&self) -> Address;
    /// Returns the destination address of the message.
    fn to(&self) -> Address;
    /// Returns the message sequence or nonce.
    fn sequence(&self) -> u64;
    /// Returns the amount sent in message.
    fn value(&self) -> TokenAmount;
    /// Returns the method number to be called.
    fn method_num(&self) -> MethodNum;
    /// Returns the encoded parameters for the method call.
    fn params(&self) -> &RawBytes;
    /// sets the gas limit for the message.
    fn set_gas_limit(&mut self, amount: u64);
    /// sets a new sequence to the message.
    fn set_sequence(&mut self, sequence: u64);
    /// Returns the gas limit for the message.
    fn gas_limit(&self) -> u64;
    /// Returns the required funds for the message.
    fn required_funds(&self) -> TokenAmount;
    /// gets gas fee cap for the message.
    fn gas_fee_cap(&self) -> TokenAmount;
    /// gets gas premium for the message.
    fn gas_premium(&self) -> TokenAmount;
    /// sets the gas fee cap.
    fn set_gas_fee_cap(&mut self, cap: TokenAmount);
    /// sets the gas premium.
    fn set_gas_premium(&mut self, prem: TokenAmount);
}

impl Message for ShimMessage {
    fn from(&self) -> Address {
        Address::from(self.from)
    }
    fn to(&self) -> Address {
        Address::from(self.to)
    }
    fn sequence(&self) -> u64 {
        self.sequence
    }
    fn value(&self) -> TokenAmount {
        TokenAmount::from(&self.value)
    }
    fn method_num(&self) -> MethodNum {
        self.method_num
    }
    fn params(&self) -> &RawBytes {
        &self.params
    }
    fn gas_limit(&self) -> u64 {
        self.gas_limit
    }
    fn set_gas_limit(&mut self, token_amount: u64) {
        self.gas_limit = token_amount;
    }
    fn set_sequence(&mut self, new_sequence: u64) {
        self.sequence = new_sequence;
    }
    fn required_funds(&self) -> TokenAmount {
        TokenAmount::from(&self.gas_fee_cap * self.gas_limit + &self.value)
    }
    fn gas_fee_cap(&self) -> TokenAmount {
        TokenAmount::from(&self.gas_fee_cap)
    }
    fn gas_premium(&self) -> TokenAmount {
        TokenAmount::from(&self.gas_premium)
    }

    fn set_gas_fee_cap(&mut self, cap: TokenAmount) {
        self.gas_fee_cap = cap.into();
    }

    fn set_gas_premium(&mut self, prem: TokenAmount) {
        self.gas_premium = prem.into();
    }
}
