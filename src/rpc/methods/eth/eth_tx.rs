// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
pub use crate::eth::{
    EthTx, EIP_1559_TX_TYPE, EIP_LEGACY_TX_TYPE, ETH_LEGACY_HOMESTEAD_TX_CHAIN_ID,
};

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
            r#type: EthUint64(EIP_1559_TX_TYPE.into()),
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
