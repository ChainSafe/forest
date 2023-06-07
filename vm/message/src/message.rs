// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_shim::{gas::Gas, message::Message, version::NetworkVersion};

/// Semantic validation and validates the message has enough gas.
pub fn valid_for_block_inclusion(
    msg: &Message,
    min_gas: Gas,
    version: NetworkVersion,
) -> Result<(), anyhow::Error> {
    use forest_shim::address::ZERO_ADDRESS;
    use fvm_shared3::{BLOCK_GAS_LIMIT, TOTAL_FILECOIN};
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
