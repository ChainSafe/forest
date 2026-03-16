// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::eth::types::{EthAddress, EthBytes, EthHash};
use crate::rpc::eth::utils::parse_eth_revert;
use crate::rpc::state::ActorTrace;
use fil_actor_evm_state::evm_shared::v17::uints::U256;

pub const ZERO_HASH: EthHash = EthHash(ethereum_types::H256([0u8; 32]));

pub fn trace_to_address(trace: &ActorTrace) -> EthAddress {
    if let Some(addr) = trace.state.delegated_address
        && let Ok(eth_addr) = EthAddress::from_filecoin_address(&addr.into())
    {
        return eth_addr;
    }
    EthAddress::from_actor_id(trace.id)
}

pub fn extract_revert_reason(output: &EthBytes) -> Option<String> {
    let reason = parse_eth_revert(&output.0);
    (!reason.starts_with("0x")).then_some(reason)
}

pub fn u256_to_eth_hash(value: &U256) -> EthHash {
    EthHash(ethereum_types::H256(value.to_big_endian()))
}
