// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::address::Address;

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct CreateExternalReturn {
    pub actor_id: u64,
    pub robust_address: Address,
    pub eth_address: [u8; 20],
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::encoding::from_slice_with_fallback;
    use fvm_ipld_encoding::to_vec;

    #[test]
    fn test_create_external_return_roundtrip() {
        let ret_struct = CreateExternalReturn {
            actor_id: 666,
            robust_address: Address::new_id(2),
            eth_address: [0; 20],
        };
        let struct_encoded = to_vec(&ret_struct).unwrap();

        let struct_decoded: CreateExternalReturn =
            from_slice_with_fallback(&struct_encoded).unwrap();
        assert_eq!(struct_decoded, ret_struct);
    }
}
