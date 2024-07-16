// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use anyhow::{ensure, Context};
use derive_builder::Builder;
use num::BigInt;
use num_bigint::Sign;
use num_traits::cast::ToPrimitive;

pub const HOMESTEAD_SIG_LEN: usize = 66;
pub const HOMESTEAD_SIG_PREFIX: u8 = 0x01;

#[derive(PartialEq, Debug, Clone, Default, Builder)]
#[builder(setter(into))]
pub struct EthLegacyHomesteadTxArgs {
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

impl EthLegacyHomesteadTxArgs {
    pub fn with_signature(mut self, signature: &Signature) -> anyhow::Result<Self> {
        ensure!(
            signature.signature_type() == SignatureType::Delegated,
            "Signature is not delegated type"
        );

        ensure!(
            signature.bytes().len() == HOMESTEAD_SIG_LEN,
            "Invalid signature length for Homestead transaction"
        );

        ensure!(
            signature.bytes().first().expect("infallible") == &HOMESTEAD_SIG_PREFIX,
            "Invalid signature prefix for Homestead transaction"
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

        let v_int = v.to_i32().context("Failed to convert v to i32")?;
        ensure!(
            v_int == 27 || v_int == 28,
            "Homestead transaction v value is invalid"
        );

        self.r = r;
        self.s = s;
        self.v = v;

        Ok(self)
    }

    pub fn rlp_signed_message(&self) -> anyhow::Result<Vec<u8>> {
        let mut stream = rlp::RlpStream::new_list(9);
        stream.append(&format_u64(self.nonce));
        stream.append(&format_bigint(&self.gas_price)?);
        stream.append(&format_u64(self.gas_limit));
        stream.append(&format_address(&self.to));
        stream.append(&format_bigint(&self.value)?);
        stream.append(&self.input);
        stream.append(&format_bigint(&self.v)?);
        stream.append(&format_bigint(&self.r)?);
        stream.append(&format_bigint(&self.s)?);

        Ok(stream.out().to_vec())
    }
}

impl EthLegacyHomesteadTxArgsBuilder {
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
