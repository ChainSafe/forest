// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::message::SignedMessage;
use crate::shim::address::Address;
use crate::shim::crypto::SignatureType::Delegated;
use anyhow::{bail, ensure, Context};
use derive_builder::Builder;
use num::{BigInt, BigUint};
use num_bigint::Sign;
use num_bigint::ToBigInt;
use num_traits::cast::ToPrimitive;
use std::ops::Mul;

pub const EIP_155_SIG_PREFIX: u8 = 0x02;

/// Description from Lotus:
/// [`EthLegacyEip155TxArgs`] is a legacy Ethereum transaction that uses the EIP-155 chain replay protection mechanism
/// by incorporating the `ChainId` in the signature.
/// See how the `V` value in the signature is derived from the `ChainId` at
/// <https://github.com/ethereum/go-ethereum/blob/86a1f0c39494c8f5caddf6bd9fbddd4bdfa944fd/core/types/transaction_signing.go#L424>
/// For [`EthLegacyEip155TxArgs`], the digest that is used to create a signed transaction includes the `ChainID` but the serialized `RLP` transaction
/// does not include the `ChainID` as an explicit field. Instead, the `ChainID` is included in the V value of the signature as mentioned above.
#[derive(PartialEq, Debug, Clone, Default, Builder)]
#[builder(setter(into))]
pub struct EthLegacyEip155TxArgs {
    pub chain_id: EthChainId,
    pub nonce: u64,
    pub gas_price: BigInt,
    pub gas_limit: u64,
    pub to: Option<EthAddress>,
    pub value: BigInt,
    pub input: Vec<u8>,
    #[builder(setter(skip))]
    pub v: BigInt,
    #[builder(setter(skip))]
    pub r: BigInt,
    #[builder(setter(skip))]
    pub s: BigInt,
}

impl EthLegacyEip155TxArgs {
    /// Returns legacy EIP-155 transaction signature
    pub fn signature(&self, eth_chain_id: EthChainId) -> anyhow::Result<Signature> {
        // Validate EIP155 Chain ID
        validate_eip155_chain_id(eth_chain_id, &self.v)?;

        // Convert r, s, v to byte arrays
        let r_bytes = self.r.to_bytes_be().1;
        let s_bytes = self.s.to_bytes_be().1;
        let v_bytes = self.v.to_bytes_be().1;

        // Initialize signature with one-byte legacy transaction marker
        let mut sig = vec![EIP_155_SIG_PREFIX];

        // Extend signature with padded r, padded s, and v
        sig.extend(pad_leading_zeros(r_bytes, 32));
        sig.extend(pad_leading_zeros(s_bytes, 32));
        sig.extend(v_bytes);

        // Check if signature length is correct
        let valid_sig_len = calc_valid_eip155_sig_len(self.chain_id);
        let sig_len = sig.len();
        ensure!(
            sig_len == valid_sig_len.0 as usize || sig_len == valid_sig_len.1 as usize,
            "signature is not {:#?} OR {:#?} bytes; it is {} bytes",
            valid_sig_len.0,
            valid_sig_len.1,
            sig_len
        );

        Ok(Signature {
            sig_type: Delegated,
            bytes: sig,
        })
    }

    /// Returns a verifiable signature for legacy EIP-155 transaction
    pub fn to_verifiable_signature(
        &self,
        mut sig: Vec<u8>,
        eth_chain_id: EthChainId,
    ) -> anyhow::Result<Vec<u8>> {
        // Check if the signature length is correct
        let valid_sig_len = calc_valid_eip155_sig_len(self.chain_id);
        ensure!(
            sig.len() == valid_sig_len.0 as usize || sig.len() == valid_sig_len.1 as usize,
            "signature should be {} or {} bytes long (1 byte metadata and rest bytes are sig data), but got {} bytes",
            valid_sig_len.0,
            valid_sig_len.1,
            sig.len()
        );

        // Check if the first byte matches the expected signature prefix
        ensure!(
            *sig.first().context("failed to get signature prefix")? == EIP_155_SIG_PREFIX,
            "expected EIP155 signature prefix 0x{:x}, but got 0x{:x}",
            EIP_155_SIG_PREFIX,
            sig.first().context("failed to get signature prefix")?
        );

        // Remove the prefix byte as it's only used for legacy transaction identification
        sig.remove(0);

        // Extract the 'v' value from the signature, which is the last byte in Ethereum signatures
        let mut v_value = BigInt::from_bytes_be(
            num_bigint::Sign::Plus,
            sig.get(64..).context("failed to get v value")?,
        );

        validate_eip155_chain_id(eth_chain_id, &v_value)?;

        let chain_id_mul = BigInt::from(eth_chain_id)
            .mul(2_i32.to_bigint().context("Failed to convert 2 to BigInt")?);
        v_value -= chain_id_mul;
        v_value -= BigInt::from(8);

        // Adjust 'v' value for compatibility with new transactions: 27 -> 0, 28 -> 1
        if v_value == BigInt::from(LEGACY_V_VALUE_27) {
            if let Some(value) = sig.get_mut(64) {
                *value = 0
            };
        } else if v_value == BigInt::from(LEGACY_V_VALUE_28) {
            if let Some(value) = sig.get_mut(64) {
                *value = 1
            };
        } else {
            bail!(
                "invalid 'v' value: expected 27 or 28, got {}",
                v_value.to_string()
            );
        }

        Ok(sig
            .get(..65)
            .context("failed to get range of values")?
            .to_vec())
    }

    pub fn with_signature(mut self, signature: &Signature) -> anyhow::Result<Self> {
        ensure!(
            signature.signature_type() == SignatureType::Delegated,
            "Signature is not delegated type"
        );

        let valid_sig_len = calc_valid_eip155_sig_len(self.chain_id);
        ensure!(
            signature.bytes().len() == valid_sig_len.0 as usize
                || signature.bytes().len() == valid_sig_len.1 as usize,
            "Invalid signature length for EIP155 transaction: {}. Must be either {} or {} bytes",
            signature.bytes().len(),
            valid_sig_len.0,
            valid_sig_len.1
        );

        // later on, we do slicing on the signature bytes, so we need to ensure that the signature is at least 1 + 65 bytes long
        ensure!(
            signature.bytes().len() >= 66,
            "Invalid signature length for EIP155 transaction: {} < 66 bytes",
            signature.bytes().len()
        );

        ensure!(
            signature.bytes().first().expect("infallible") == &EIP_155_SIG_PREFIX,
            "Invalid signature prefix for EIP155 transaction"
        );

        // ignore the first byte of the signature as it's only used for legacy transaction identification
        let r = BigInt::from_bytes_be(
            Sign::Plus,
            signature.bytes().get(1..33).expect("infallible"),
        );
        let s = BigInt::from_bytes_be(
            Sign::Plus,
            signature.bytes().get(33..65).expect("infallible"),
        );
        let v = BigInt::from_bytes_be(Sign::Plus, signature.bytes().get(65..).expect("infallible"));

        validate_eip155_chain_id(self.chain_id, &v)?;

        self.r = r;
        self.s = s;
        self.v = v;

        Ok(self)
    }

    /// Returns an RLP stream representing the legacy EIP-155 transaction message
    fn message_rlp_stream(&self) -> anyhow::Result<rlp::RlpStream> {
        let mut stream = rlp::RlpStream::new();
        stream
            .begin_unbounded_list()
            .append(&format_u64(self.nonce))
            .append(&format_bigint(&self.gas_price)?)
            .append(&format_u64(self.gas_limit))
            .append(&format_address(&self.to))
            .append(&format_bigint(&self.value)?)
            .append(&self.input);
        Ok(stream)
    }

    /// Returns the signed RLP-encoded message for the legacy EIP-155 transaction
    pub fn rlp_signed_message(&self) -> anyhow::Result<Vec<u8>> {
        let mut stream = self.message_rlp_stream()?;
        stream
            .append(&format_bigint(&self.v)?)
            .append(&format_bigint(&self.r)?)
            .append(&format_bigint(&self.s)?)
            .finalize_unbounded_list();
        Ok(stream.out().to_vec())
    }

    /// Returns the unsigned RLP-encoded message for the Legacy EIP-155 transaction
    pub fn rlp_unsigned_message(&self, eth_chain_id: EthChainId) -> anyhow::Result<Vec<u8>> {
        let mut stream = self.message_rlp_stream()?;
        stream
            .append(&format_bigint(&BigInt::from(eth_chain_id))?)
            .append(&format_u64(0))
            .append(&format_u64(0))
            .finalize_unbounded_list();
        Ok(stream.out().to_vec())
    }

    /// Constructs a signed message using legacy EIP-155 transaction args
    pub fn get_signed_message(
        &self,
        from: Address,
        eth_chain_id: EthChainId,
    ) -> anyhow::Result<SignedMessage> {
        let message = self.get_unsigned_message(from, eth_chain_id)?;
        let signature = self.signature(eth_chain_id)?;
        Ok(SignedMessage { message, signature })
    }

    /// Constructs an unsigned message using legacy EIP-155 transaction args
    pub fn get_unsigned_message(
        &self,
        from: Address,
        eth_chain_id: EthChainId,
    ) -> anyhow::Result<Message> {
        ensure!(
            validate_eip155_chain_id(eth_chain_id, &self.v).is_ok(),
            "Failed to validate EIP155 chain Id"
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
            gas_fee_cap: self.gas_price.clone().into(),
            gas_premium: self.gas_price.clone().into(),
        })
    }
}

impl EthLegacyEip155TxArgsBuilder {
    pub fn unsigned_message(&mut self, message: &Message) -> anyhow::Result<&mut Self> {
        let (params, to) = get_eth_params_and_recipient(message)?;
        Ok(self
            .nonce(message.sequence)
            .value(message.value.clone())
            .gas_price(message.gas_fee_cap.clone())
            .gas_limit(message.gas_limit)
            .to(to)
            .input(params))
    }
}

/// Validates the EIP155 chain ID by deriving it from the given `v` value
pub fn validate_eip155_chain_id(eth_chain_id: EthChainId, v: &BigInt) -> anyhow::Result<()> {
    let derived_chain_id = derive_eip_155_chain_id(v)?;
    ensure!(
        derived_chain_id
            .to_u64()
            .context("unable to convert derived chain to `u64`")?
            == eth_chain_id,
        "EIP155 transaction chain ID mismatch: expected {eth_chain_id}, got {derived_chain_id}",
    );

    Ok(())
}

/// Derives the EIP155 chain ID from the `V` value
pub fn derive_eip_155_chain_id(v: &BigInt) -> anyhow::Result<BigInt> {
    if v.bits() <= 64 {
        let v = v.to_u64().context("Failed to convert v to u64")?;
        if v == 27 || v == 28 {
            return Ok(0.into());
        }
        return Ok(((v - 35) / 2).into());
    }

    Ok((v - 35u32) / 2u32)
}

pub(super) fn calc_eip155_sig_len(eth_chain_id: EthChainId, v: u64) -> u64 {
    let chain_id = BigUint::from(eth_chain_id);
    let v: BigUint = chain_id * 2u64 + v;
    let v_len = v.to_bytes_be().len() as u64;

    // EthLegacyHomesteadTxSignatureLen includes the 1 byte legacy tx marker prefix and also 1 byte for the V value.
    // So we subtract 1 to not double count the length of the v value
    HOMESTEAD_SIG_LEN as u64 + v_len - 1u64
}

/// Returns the valid signature lengths for EIP-155 transactions.
/// The length is based on the chain ID and the V value in the signature.
pub fn calc_valid_eip155_sig_len(eth_chain_id: EthChainId) -> (u64, u64) {
    let sig_len1 = calc_eip155_sig_len(eth_chain_id, 35);
    let sig_len2 = calc_eip155_sig_len(eth_chain_id, 36);
    (sig_len1, sig_len2)
}

#[cfg(test)]
mod tests {
    use num_bigint::ToBigInt;
    use quickcheck_macros::quickcheck;

    use super::*;

    #[quickcheck]
    fn test_derive_eip_155_chain_id(eth_chain_id: EthChainId) {
        let eth_chain_id = eth_chain_id.to_bigint().unwrap();
        let v = (eth_chain_id.clone() * 2.to_bigint().unwrap() + 35.to_bigint().unwrap())
            .to_bytes_be()
            .1;
        assert_eq!(
            derive_eip_155_chain_id(&BigInt::from_bytes_be(Sign::Plus, &v)).unwrap(),
            eth_chain_id
        );
    }

    #[quickcheck]
    fn test_validate_eip155_chain_id(eth_chain_id: EthChainId) {
        let eth_chain_id = eth_chain_id.to_bigint().unwrap();
        let v = (eth_chain_id.clone() * 2.to_bigint().unwrap() + 35.to_bigint().unwrap())
            .to_bytes_be()
            .1;
        validate_eip155_chain_id(
            eth_chain_id.clone().to_u64().unwrap(),
            &BigInt::from_bytes_be(Sign::Plus, &v),
        )
        .unwrap();
    }

    #[test]
    fn test_calc_eip_155_sig_len() {
        let cases = [
            (
                "ChainId fits in 1 byte",
                0x01,
                HOMESTEAD_SIG_LEN as u64 + 1 - 1,
            ),
            (
                "ChainId fits in 2 bytes",
                0x0100,
                HOMESTEAD_SIG_LEN as u64 + 2 - 1,
            ),
            (
                "ChainId fits in 3 bytes",
                0x10000,
                HOMESTEAD_SIG_LEN as u64 + 3 - 1,
            ),
            (
                "ChainId fits in 4 bytes",
                0x01000000,
                HOMESTEAD_SIG_LEN as u64 + 4 - 1,
            ),
            (
                "ChainId fits in 6 bytes",
                0x010000000000,
                HOMESTEAD_SIG_LEN as u64 + 6 - 1,
            ),
        ];

        for (name, chain_id, expected) in cases {
            let actual = calc_eip155_sig_len(chain_id, 1);
            assert_eq!(actual, expected, "{name}");
        }
    }
}
