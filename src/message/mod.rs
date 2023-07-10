// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod chain_message;
pub mod signed_message;

use crate::shim::{address::Address, econ::TokenAmount, message::Message as ShimMessage};
use crate::shim::{gas::Gas, version::NetworkVersion};
pub use chain_message::ChainMessage;
use fvm_ipld_encoding::RawBytes;
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
        self.from
    }
    fn to(&self) -> Address {
        self.to
    }
    fn sequence(&self) -> u64 {
        self.sequence
    }
    fn value(&self) -> TokenAmount {
        self.value.clone()
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
        &self.gas_fee_cap * self.gas_limit + &self.value
    }
    fn gas_fee_cap(&self) -> TokenAmount {
        self.gas_fee_cap.clone()
    }
    fn gas_premium(&self) -> TokenAmount {
        self.gas_premium.clone()
    }

    fn set_gas_fee_cap(&mut self, cap: TokenAmount) {
        self.gas_fee_cap = cap;
    }

    fn set_gas_premium(&mut self, prem: TokenAmount) {
        self.gas_premium = prem;
    }
}

/// Semantic validation and validates the message has enough gas.
pub fn valid_for_block_inclusion(
    msg: &ShimMessage,
    min_gas: Gas,
    version: NetworkVersion,
) -> Result<(), anyhow::Error> {
    use crate::shim::address::ZERO_ADDRESS;
    use crate::shim::econ::{BLOCK_GAS_LIMIT, TOTAL_FILECOIN};
    if msg.version != 0 {
        anyhow::bail!("Message version: {} not supported", msg.version);
    }
    if msg.to == *ZERO_ADDRESS && version >= NetworkVersion::V7 {
        anyhow::bail!("invalid 'to' address");
    }
    if msg.value.is_negative() {
        anyhow::bail!("message value cannot be negative");
    }
    if msg.value > *TOTAL_FILECOIN {
        anyhow::bail!("message value cannot be greater than total FIL supply");
    }
    if msg.gas_fee_cap.is_negative() {
        anyhow::bail!("gas_fee_cap cannot be negative");
    }
    if msg.gas_premium.is_negative() {
        anyhow::bail!("gas_premium cannot be negative");
    }
    if msg.gas_premium > msg.gas_fee_cap {
        anyhow::bail!("gas_fee_cap less than gas_premium");
    }
    if msg.gas_limit > BLOCK_GAS_LIMIT {
        anyhow::bail!(
            "gas_limit {} cannot be greater than block gas limit",
            msg.gas_limit
        );
    }

    if Gas::new(msg.gas_limit) < min_gas {
        anyhow::bail!(
            "gas_limit {} cannot be less than cost {} of storing a message on chain",
            msg.gas_limit,
            min_gas
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    mod builder_test;
}
