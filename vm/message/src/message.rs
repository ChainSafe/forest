// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm::gas::Gas;
use fvm_shared::message::Message;

/// Semantic validation and validates the message has enough gas.
#[cfg(feature = "proofs")]
pub fn valid_for_block_inclusion(
    msg: &Message,
    min_gas: Gas,
    version: fil_types::NetworkVersion,
) -> Result<(), String> {
    use fil_types::{NetworkVersion, BLOCK_GAS_LIMIT, TOTAL_FILECOIN, ZERO_ADDRESS};
    use num_traits::Signed;
    if msg.version != 0 {
        return Err(format!("Message version: {} not supported", msg.version));
    }
    if msg.to == *ZERO_ADDRESS && version >= NetworkVersion::V7 {
        return Err("invalid 'to' address".to_string());
    }
    if msg.value.is_negative() {
        return Err("message value cannot be negative".to_string());
    }
    if msg.value > *TOTAL_FILECOIN {
        return Err("message value cannot be greater than total FIL supply".to_string());
    }
    if msg.gas_fee_cap.is_negative() {
        return Err("gas_fee_cap cannot be negative".to_string());
    }
    if msg.gas_premium.is_negative() {
        return Err("gas_premium cannot be negative".to_string());
    }
    if msg.gas_premium > msg.gas_fee_cap {
        return Err("gas_fee_cap less than gas_premium".to_string());
    }
    if msg.gas_limit > BLOCK_GAS_LIMIT {
        return Err("gas_limit cannot be greater than block gas limit".to_string());
    }
    if Gas::new(msg.gas_limit) < min_gas {
        return Err(format!(
            "gas_limit {} cannot be less than cost {} of storing a message on chain",
            msg.gas_limit, min_gas
        ));
    }

    Ok(())
}
