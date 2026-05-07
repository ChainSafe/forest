// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::eth::types::{EthAddress, EthBytes, EthHash};
use crate::rpc::eth::utils::parse_eth_revert;
use crate::rpc::state::ActorTrace;
use fil_actor_evm_state::evm_shared::v17::uints::U256;

/// The zero-valued `EthHash`, used as a sentinel for cleared storage slots.   
pub const ZERO_HASH: EthHash = EthHash(ethereum_types::H256([0u8; 32]));

/// Resolves an actor trace to its Ethereum address.
/// Prefers the delegated (0x) address when available, falls back to a masked actor ID.
pub fn trace_to_address(trace: &ActorTrace) -> EthAddress {
    if let Some(addr) = trace.state.delegated_address
        && let Ok(eth_addr) = EthAddress::from_filecoin_address(&addr.into())
    {
        return eth_addr;
    }
    EthAddress::from_actor_id(trace.id)
}

/// Parses a human-readable revert reason from EVM output bytes.
/// Returns `None` if the output cannot be decoded or is a raw hex string.
pub fn extract_revert_reason(output: &EthBytes) -> Option<String> {
    let reason = parse_eth_revert(&output.0);
    (!reason.starts_with("0x")).then_some(reason)
}

/// Converts a `U256` value to an `EthHash` using big-endian byte order.                                                                                                                                             
pub fn u256_to_eth_hash(value: &U256) -> EthHash {
    EthHash(ethereum_types::H256(value.to_big_endian()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u256_to_eth_hash_zero() {
        let zero = U256::from(0u64);
        assert_eq!(u256_to_eth_hash(&zero), ZERO_HASH);
    }

    #[test]
    fn test_u256_to_eth_hash_nonzero() {
        let val = U256::from(1u64);
        let hash = u256_to_eth_hash(&val);
        // Big-endian: value 1 should be in the last byte
        assert_eq!(hash.0.0[31], 1);
        assert_eq!(hash.0.0[0], 0);
    }

    #[test]
    fn test_u256_to_eth_hash_max() {
        let max = U256::MAX;
        let hash = u256_to_eth_hash(&max);
        assert!(hash.0.0.iter().all(|&b| b == 0xff));
    }

    #[test]
    fn test_extract_revert_reason_empty() {
        assert!(extract_revert_reason(&EthBytes(vec![])).is_none());
    }

    #[test]
    fn test_extract_revert_reason_hex_passthrough() {
        // parse_eth_revert returns "0x..." for unrecognized selectors,
        // which extract_revert_reason filters out.
        let unknown = EthBytes(vec![0xde, 0xad, 0xbe, 0xef]);
        assert!(extract_revert_reason(&unknown).is_none());
    }

    #[test]
    fn test_zero_hash_is_all_zeros() {
        assert_eq!(ZERO_HASH.0, ethereum_types::H256([0u8; 32]));
    }
}
