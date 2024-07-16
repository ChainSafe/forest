// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::eth::EthTx;

use super::*;

impl ApiEthTx {
    pub fn eth_hash(&self) -> Result<Hash> {
        Ok(Hash(keccak(self.rlp_signed_message()?)))
    }

    pub fn rlp_signed_message(&self) -> Result<Vec<u8>> {
        let stream = match self.r#type.0 {
            EIP_1559_TX_TYPE => {
                let mut stream = RlpStream::new_list(12);
                stream.append(&format_u64(self.chain_id.0));
                stream.append(&format_u64(self.nonce.0));
                stream.append(&format_bigint(
                    self.max_priority_fee_per_gas.as_ref().with_context(|| {
                        format!(
                            "max_priority_fee_per_gas is required for type {}",
                            self.r#type.0,
                        )
                    })?,
                )?);
                stream.append(&format_bigint(
                    self.max_fee_per_gas.as_ref().with_context(|| {
                        format!("max_fee_per_gas is required for type {}", self.r#type.0)
                    })?,
                )?);
                stream.append(&format_u64(self.gas.0));
                stream.append(&format_address(&self.to));
                stream.append(&format_bigint(&self.value)?);
                stream.append(&self.input.0);
                let access_list: &[u8] = &[];
                stream.append_list(access_list);

                stream.append(&format_bigint(&self.v)?);
                stream.append(&format_bigint(&self.r)?);
                stream.append(&format_bigint(&self.s)?);
                stream
            }
            EIP_LEGACY_TX_TYPE => {
                let mut stream = RlpStream::new_list(9);
                stream.append(&format_u64(self.nonce.0));
                stream.append(&format_bigint(self.gas_price.as_ref().with_context(
                    || format!("gas_price is required for type {}", self.r#type.0),
                )?)?);
                stream.append(&format_u64(self.gas.0));
                stream.append(&format_address(&self.to));
                stream.append(&format_bigint(&self.value)?);
                stream.append(&self.input.0);
                stream.append(&format_bigint(&self.v)?);
                stream.append(&format_bigint(&self.r)?);
                stream.append(&format_bigint(&self.s)?);
                stream
            }
            t => anyhow::bail!("unsupported type {t}"),
        };

        let mut rlp = stream.out().to_vec();
        let mut bytes: Vec<u8> = if self.r#type.0 == EIP_1559_TX_TYPE {
            vec![EIP_1559_TX_TYPE.try_into()?]
        } else {
            vec![]
        };
        bytes.append(&mut rlp);

        let hex = bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join("");
        tracing::trace!("rlp: {}", &hex);

        Ok(bytes)
    }
}

impl From<EthLegacyHomesteadTxArgs> for ApiEthTx {
    fn from(
        EthLegacyHomesteadTxArgs {
            nonce,
            gas_price,
            gas_limit,
            to,
            value,
            input,
            v,
            r,
            s,
        }: EthLegacyHomesteadTxArgs,
    ) -> Self {
        Self {
            chain_id: ETH_LEGACY_HOMESTEAD_TX_CHAIN_ID.into(),
            r#type: EIP_LEGACY_TX_TYPE.into(),
            nonce: nonce.into(),
            gas_price: Some(gas_price.into()),
            gas: gas_limit.into(),
            to,
            value: value.into(),
            input: input.into(),
            v: v.into(),
            r: r.into(),
            s: s.into(),
            ..Default::default()
        }
    }
}

impl From<EthLegacyEip155TxArgs> for ApiEthTx {
    fn from(
        EthLegacyEip155TxArgs {
            chain_id,
            nonce,
            gas_price,
            gas_limit,
            to,
            value,
            input,
            v,
            r,
            s,
        }: EthLegacyEip155TxArgs,
    ) -> Self {
        Self {
            chain_id: chain_id.into(),
            r#type: EIP_LEGACY_TX_TYPE.into(),
            nonce: nonce.into(),
            gas_price: Some(gas_price.into()),
            gas: gas_limit.into(),
            to,
            value: value.into(),
            input: input.into(),
            v: v.into(),
            r: r.into(),
            s: s.into(),
            ..Default::default()
        }
    }
}

impl From<EthEip1559TxArgs> for ApiEthTx {
    fn from(
        EthEip1559TxArgs {
            chain_id,
            nonce,
            to,
            value,
            max_fee_per_gas,
            max_priority_fee_per_gas,
            gas_limit,
            input,
            v,
            r,
            s,
        }: EthEip1559TxArgs,
    ) -> Self {
        Self {
            chain_id: chain_id.into(),
            r#type: EIP_1559_TX_TYPE.into(),
            nonce: nonce.into(),
            gas: gas_limit.into(),
            to,
            value: value.into(),
            max_fee_per_gas: Some(max_fee_per_gas.into()),
            max_priority_fee_per_gas: Some(max_priority_fee_per_gas.into()),
            input: input.into(),
            v: v.into(),
            r: r.into(),
            s: s.into(),
            ..Default::default()
        }
    }
}

impl From<EthTx> for ApiEthTx {
    fn from(value: EthTx) -> Self {
        use EthTx::*;
        match value {
            Homestead(tx) => (*tx).into(),
            Eip1559(tx) => (*tx).into(),
            Eip155(tx) => (*tx).into(),
        }
    }
}
