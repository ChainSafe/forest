// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm::gas::Gas;
use fvm_shared::message::Message;

/// Semantic validation and validates the message has enough gas.
pub fn valid_for_block_inclusion(
    msg: &Message,
    min_gas: Gas,
    version: fvm_shared::version::NetworkVersion,
) -> Result<(), anyhow::Error> {
    use fvm_shared::version::NetworkVersion;
    use fvm_shared::{BLOCK_GAS_LIMIT, TOTAL_FILECOIN, ZERO_ADDRESS};
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
        anyhow::bail!("gas_limit cannot be greater than block gas limit");
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
