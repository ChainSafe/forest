// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::bail;
use num::BigInt;
use num_bigint::Sign;

// TODO: this should not live in RPC but in the ETH module.
use crate::{rpc::eth::types::EthAddress, shim::crypto::Signature};

use super::EthChainId;
pub const EIP_1559_SIG_LEN: usize = 65;

#[derive(PartialEq, Debug, Clone, Default)]
pub struct EthEip1559TxArgs {
    pub chain_id: EthChainId,
    pub nonce: u64,
    pub to: Option<EthAddress>,
    pub value: BigInt,
    pub max_fee_per_gas: BigInt,
    pub max_priority_fee_per_gas: BigInt,
    pub gas_limit: u64,
    pub input: Vec<u8>,
    pub v: BigInt,
    pub r: BigInt,
    pub s: BigInt,
}
impl EthEip1559TxArgs {
    // TODO: make fluent API
    pub fn initialise_signature(&mut self, signature: &Signature) -> anyhow::Result<()> {
        if signature.signature_type() != crate::shim::crypto::SignatureType::Delegated {
            bail!(
                "Signature is not delegated type, is {}",
                signature.signature_type()
            );
        }

        if signature.bytes().len() != EIP_1559_SIG_LEN {
            bail!(
                "Invalid signature length for EIP1559 transaction: {}",
                signature.bytes().len()
            );
        }

        self.r = BigInt::from_bytes_be(Sign::Plus, &signature.bytes()[0..32]);
        self.s = BigInt::from_bytes_be(Sign::Plus, &signature.bytes()[32..64]);
        self.v = BigInt::from_bytes_be(Sign::Plus, &signature.bytes()[64..=EIP_1559_SIG_LEN]);

        Ok(())
    }
}
