// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    rpc::eth::types::EthAddress,
    shim::crypto::{Signature, SignatureType},
};
use anyhow::{ensure, Context};
use derive_builder::Builder;
use num::{BigInt, BigUint};
use num_bigint::Sign;
use num_traits::cast::ToPrimitive;

use super::{homestead_transaction::HOMESTEAD_SIG_LEN, EthChainId};

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
    pub(crate) fn with_signature(
        mut self,
        signature: &Signature,
        eth_chain_id: EthChainId,
    ) -> anyhow::Result<Self> {
        ensure!(
            signature.signature_type() == SignatureType::Delegated,
            "Signature is not delegated type"
        );

        let valid_sig_len = calc_valid_eip155_sig_len(eth_chain_id);
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

        validate_eip155_chain_id(eth_chain_id, &v)?;

        self.r = r;
        self.s = s;
        self.v = v;

        Ok(self)
    }
}

fn validate_eip155_chain_id(eth_chain_id: EthChainId, v: &BigInt) -> anyhow::Result<()> {
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

fn derive_eip_155_chain_id(v: &BigInt) -> anyhow::Result<BigInt> {
    ensure!(v >= &35.into(), "Invalid V value for EIP155 transaction");

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
pub(super) fn calc_valid_eip155_sig_len(eth_chain_id: EthChainId) -> (u64, u64) {
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
