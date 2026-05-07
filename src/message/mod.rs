// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod chain_message;
pub mod signed_message;

use crate::shim::message::MethodNum;
use crate::shim::{address::Address, econ::TokenAmount, message::Message};
use crate::shim::{gas::Gas, version::NetworkVersion};
use ambassador::delegatable_trait;
pub use chain_message::ChainMessage;
use fvm_ipld_encoding::RawBytes;
use num::Zero;
pub use signed_message::SignedMessage;

/// Message interface to make read-only interactions with Signed and unsigned messages in a generic
/// context.
#[auto_impl::auto_impl(&, Arc)]
#[delegatable_trait]
pub trait MessageRead {
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
    /// Returns the gas limit for the message.
    fn gas_limit(&self) -> u64;
    /// Returns the required funds for the message.
    fn required_funds(&self) -> TokenAmount;
    /// gets gas fee cap for the message.
    fn gas_fee_cap(&self) -> TokenAmount;
    /// gets gas premium for the message.
    fn gas_premium(&self) -> TokenAmount;
    /// This method returns the effective gas premium claimable by the miner
    /// given the supplied base fee. This method is not used anywhere except the `Eth` API.
    ///
    /// Filecoin clamps the gas premium at `gas_fee_cap` - `base_fee`, if lower than the
    /// specified premium. Returns 0 if `gas_fee_cap` is less than `base_fee`.
    fn effective_gas_premium(&self, base_fee: &TokenAmount) -> TokenAmount {
        let available = self.gas_fee_cap() - base_fee;
        // It's possible that storage providers may include messages with gasFeeCap less than the baseFee
        // In such cases, their reward should be viewed as zero
        available.clamp(TokenAmount::zero(), self.gas_premium())
    }
}

/// Message interface to interact with Signed and unsigned messages in a generic
/// context.
pub trait MessageReadWrite: MessageRead {
    /// sets the gas limit for the message.
    fn set_gas_limit(&mut self, amount: u64);
    /// sets a new sequence to the message.
    fn set_sequence(&mut self, sequence: u64);
    /// sets the gas fee cap.
    fn set_gas_fee_cap(&mut self, cap: TokenAmount);
    /// sets the gas premium.
    fn set_gas_premium(&mut self, prem: TokenAmount);
}

impl MessageRead for Message {
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
    fn required_funds(&self) -> TokenAmount {
        &self.gas_fee_cap * self.gas_limit
    }
    fn gas_fee_cap(&self) -> TokenAmount {
        self.gas_fee_cap.clone()
    }
    fn gas_premium(&self) -> TokenAmount {
        self.gas_premium.clone()
    }
}

impl MessageReadWrite for Message {
    fn set_gas_limit(&mut self, token_amount: u64) {
        self.gas_limit = token_amount;
    }
    fn set_sequence(&mut self, new_sequence: u64) {
        self.sequence = new_sequence;
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
    msg: &Message,
    min_gas: Gas,
    version: NetworkVersion,
) -> anyhow::Result<()> {
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

    use itertools::Itertools;

    use super::*;

    #[test]
    fn test_effective_gas_premium() {
        // Test cases from the FIP-0115
        // <https://github.com/filecoin-project/FIPs/blob/b84b89a34ccb3d239493392a7867d6b082193b38/FIPS/fip-0115.md#premium>>
        let test_cases = vec![
            // (base_fee, gas_fee_cap, gas_premium, expected)
            (8, 8, 8, 0),
            (8, 16, 7, 7),
            (8, 19, 10, 10),
            (123456, 123455, 123455, 0),
            (123456, 1234567, 1111112, 1111111),
        ]
        .into_iter()
        .map(|(base_fee, gas_fee_cap, gas_premium, expected)| {
            (
                TokenAmount::from_atto(base_fee),
                TokenAmount::from_atto(gas_fee_cap),
                TokenAmount::from_atto(gas_premium),
                TokenAmount::from_atto(expected),
            )
        })
        .collect_vec();

        for (base_fee, gas_fee_cap, gas_premium, expected) in test_cases.into_iter() {
            let msg = Message {
                gas_fee_cap: gas_fee_cap.clone(),
                gas_premium: gas_premium.clone(),
                ..Default::default()
            };

            let result = msg.effective_gas_premium(&base_fee);
            assert_eq!(
                result, expected,
                "base_fee={} gas_fee_cap={} gas_premium={} expected={} got={}",
                base_fee, gas_fee_cap, gas_premium, expected, result
            );
        }
    }
}
