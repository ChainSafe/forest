// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the logic for EIP-1559 transaction types.
//! Constants are taken from [FIP-0091](https://github.com/filecoin-project/FIPs/blob/020bcb412ee20a2879b4a710337959c51b938d3b/FIPS/fip-0091.md).

use anyhow::ensure;
use derive_builder::Builder;
use num::BigInt;
use num_bigint::Sign;

use super::*;

pub const EIP_1559_SIG_LEN: usize = 65;

#[derive(PartialEq, Debug, Clone, Default, Builder)]
#[builder(setter(into))]
pub struct EthEip1559TxArgs {
    pub chain_id: EthChainId,
    pub nonce: u64,
    pub to: Option<EthAddress>,
    pub value: BigInt,
    pub max_fee_per_gas: BigInt,
    pub max_priority_fee_per_gas: BigInt,
    pub gas_limit: u64,
    pub input: Vec<u8>,
    #[builder(setter(skip))]
    pub v: BigInt,
    #[builder(setter(skip))]
    pub r: BigInt,
    #[builder(setter(skip))]
    pub s: BigInt,
}

impl EthEip1559TxArgs {
    pub fn with_signature(mut self, signature: &Signature) -> anyhow::Result<Self> {
        ensure!(
            signature.signature_type() == crate::shim::crypto::SignatureType::Delegated,
            "Signature is not delegated type, is {}",
            signature.signature_type()
        );

        ensure!(
            signature.bytes().len() == EIP_1559_SIG_LEN,
            "Invalid signature length for EIP1559 transaction: {}",
            signature.bytes().len()
        );

        self.r = BigInt::from_bytes_be(Sign::Plus, signature.bytes().get(..32).expect("infalible"));
        self.s = BigInt::from_bytes_be(
            Sign::Plus,
            signature.bytes().get(32..64).expect("infalible"),
        );
        self.v = BigInt::from_bytes_be(
            Sign::Plus,
            signature
                .bytes()
                .get(64..EIP_1559_SIG_LEN)
                .expect("infalible"),
        );

        Ok(self)
    }

    pub fn rlp_signed_message(&self) -> anyhow::Result<Vec<u8>> {
        // https://github.com/filecoin-project/lotus/blob/v1.27.1/chain/types/ethtypes/eth_1559_transactions.go#L72
        let prefix = [EIP_1559_TX_TYPE as u8].as_slice();
        let access_list: &[u8] = &[];
        let mut stream = rlp::RlpStream::new_with_buffer(prefix.into());
        stream
            .begin_unbounded_list()
            .append(&format_u64(self.chain_id))
            .append(&format_u64(self.nonce))
            .append(&format_bigint(&self.max_priority_fee_per_gas)?)
            .append(&format_bigint(&self.max_fee_per_gas)?)
            .append(&format_u64(self.gas_limit))
            .append(&format_address(&self.to))
            .append(&format_bigint(&self.value)?)
            .append(&self.input)
            .append_list(access_list)
            .append(&format_bigint(&self.v)?)
            .append(&format_bigint(&self.r)?)
            .append(&format_bigint(&self.s)?)
            .finalize_unbounded_list();
        Ok(stream.out().to_vec())
    }
}

impl EthEip1559TxArgsBuilder {
    pub fn unsigned_message(&mut self, message: &Message) -> anyhow::Result<&mut Self> {
        let (params, to) = get_eth_params_and_recipient(message)?;
        Ok(self
            .nonce(message.sequence)
            .value(message.value.clone())
            .max_fee_per_gas(message.gas_fee_cap.clone())
            .max_priority_fee_per_gas(message.gas_premium.clone())
            .gas_limit(message.gas_limit)
            .to(to)
            .input(params))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shim::crypto::{Signature, SignatureType};
    use num_bigint::ToBigInt;

    fn create_eip1559_tx_args() -> EthEip1559TxArgs {
        EthEip1559TxArgsBuilder::default()
            .chain_id(42u64)
            .nonce(1u64)
            .to(Some(EthAddress::default()))
            .value(100.to_bigint().unwrap())
            .max_fee_per_gas(10.to_bigint().unwrap())
            .max_priority_fee_per_gas(1.to_bigint().unwrap())
            .gas_limit(1000u64)
            .input(vec![1, 2, 3])
            .build()
            .unwrap()
    }

    // TODO(forest): https://github.com/ChainSafe/forest/issues/4478
    // Grab better test vectors, e.g., from Lotus:
    // <https://github.com/filecoin-project/lotus/blob/85abc61c17bfbddd92b1e568dee83da1c3127bc9/chain/types/ethtypes/eth_1559_transactions_test.go>
    // This will require implementing more methods for parsing different Ethereum transaction
    // types.

    #[test]
    fn test_valid_eip1559_tx_args_with_signature() {
        let args = create_eip1559_tx_args();
        let signature = Signature::new(SignatureType::Delegated, vec![0u8; EIP_1559_SIG_LEN]);
        args.with_signature(&signature).unwrap();
    }

    #[test]
    fn test_invalid_eip1559_tx_args_not_delegated() {
        let args = create_eip1559_tx_args();
        let signature = Signature::new(SignatureType::Secp256k1, vec![0u8; EIP_1559_SIG_LEN]);
        assert!(args.with_signature(&signature).is_err());
    }

    #[test]
    fn test_invalid_eip1559_tx_args_invalid_signature_len() {
        let args = create_eip1559_tx_args();
        let signature = Signature::new(SignatureType::Delegated, vec![0u8; EIP_1559_SIG_LEN - 1]);
        assert!(args.with_signature(&signature).is_err());
    }
}
