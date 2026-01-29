// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module contains the logic for EIP-1559 transaction types.
//! Constants are taken from [FIP-0091](https://github.com/filecoin-project/FIPs/blob/020bcb412ee20a2879b4a710337959c51b938d3b/FIPS/fip-0091.md).

use crate::message::SignedMessage;
use crate::shim::address::Address;
use crate::shim::crypto::SignatureType::Delegated;
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
    /// Returns EIP-1559 transaction signature
    pub fn signature(&self) -> anyhow::Result<Signature> {
        // Convert r, s, v to byte arrays
        let r_bytes = self.r.to_bytes_be().1;
        let s_bytes = self.s.to_bytes_be().1;
        let v_bytes = self.v.to_bytes_be().1;

        // Pad r and s to 32 bytes
        let mut sig = pad_leading_zeros(r_bytes, 32);
        sig.extend(pad_leading_zeros(s_bytes, 32));

        if v_bytes.is_empty() {
            sig.push(0);
        } else {
            sig.push(*v_bytes.first().expect("failed to get first byte of V"));
        }

        ensure!(sig.len() == EIP_1559_SIG_LEN, "invalid signature length");

        Ok(Signature {
            sig_type: Delegated,
            bytes: sig,
        })
    }

    /// Returns a verifiable signature for EIP-1559 transaction
    pub fn to_verifiable_signature(&self, sig: Vec<u8>) -> anyhow::Result<Vec<u8>> {
        Ok(sig)
    }

    pub fn with_signature(mut self, signature: &Signature) -> anyhow::Result<Self> {
        ensure!(
            signature.signature_type() == Delegated,
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

    /// Returns an RLP stream representing the EIP-1559 transaction message
    fn message_rlp_stream(&self) -> anyhow::Result<rlp::RlpStream> {
        // https://github.com/filecoin-project/lotus/blob/v1.27.1/chain/types/ethtypes/eth_1559_transactions.go#L72
        let prefix = [EIP_1559_TX_TYPE].as_slice();
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
            .append_list(access_list);
        Ok(stream)
    }

    /// Returns the signed RLP-encoded message for the EIP-1559 transaction
    pub fn rlp_signed_message(&self) -> anyhow::Result<Vec<u8>> {
        let mut stream = self.message_rlp_stream()?;
        stream
            .append(&format_bigint(&self.v)?)
            .append(&format_bigint(&self.r)?)
            .append(&format_bigint(&self.s)?)
            .finalize_unbounded_list();
        Ok(stream.out().to_vec())
    }

    /// Returns the unsigned RLP-encoded message for the EIP-1559 transaction
    pub fn rlp_unsigned_message(&self) -> anyhow::Result<Vec<u8>> {
        let mut stream = self.message_rlp_stream()?;
        stream.finalize_unbounded_list();
        Ok(stream.out().to_vec())
    }

    /// Constructs a signed message using EIP-1559 transaction args
    pub fn get_signed_message(
        &self,
        from: Address,
        eth_chain_id: EthChainId,
    ) -> anyhow::Result<SignedMessage> {
        let message = self.get_unsigned_message(from, eth_chain_id)?;
        let signature = self.signature()?;
        Ok(SignedMessage { message, signature })
    }

    /// Constructs an unsigned message using EIP-1559 transaction args
    pub fn get_unsigned_message(
        &self,
        from: Address,
        eth_chain_id: EthChainId,
    ) -> anyhow::Result<Message> {
        ensure!(
            self.chain_id == eth_chain_id,
            "Invalid chain id, expected {}, got {}",
            self.chain_id,
            eth_chain_id
        );
        let method_info = get_filecoin_method_info(&self.to, &self.input)?;
        Ok(Message {
            version: 0,
            from,
            to: method_info.to,
            sequence: self.nonce,
            value: self.value.clone().into(),
            method_num: method_info.method,
            params: method_info.params.into(),
            gas_limit: self.gas_limit,
            gas_fee_cap: self.max_fee_per_gas.clone().into(),
            gas_premium: self.max_priority_fee_per_gas.clone().into(),
        })
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

    #[test]
    fn test_signature() {
        let args = create_eip1559_tx_args();
        let signature = Signature::new(SignatureType::Delegated, vec![0u8; EIP_1559_SIG_LEN]);
        args.clone().with_signature(&signature).unwrap();

        let sig = args.signature().unwrap();
        assert_eq!(sig, signature);
        assert!(args.to_verifiable_signature(sig.bytes().to_vec()).is_ok());
    }
}
