// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use crate::shim::{
    address::Address,
    econ::TokenAmount,
    message::{Message, Message_v4},
};
use fvm_ipld_encoding::RawBytes;
use ::cid::Cid;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "Message")]
pub struct MessageLotusJson {
    #[serde(default)]
    version: u64,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    to: Address,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    from: Address,
    #[serde(default)]
    nonce: u64,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json", default)]
    value: TokenAmount,
    #[serde(default)]
    gas_limit: u64,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json", default)]
    gas_fee_cap: TokenAmount,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json", default)]
    gas_premium: TokenAmount,
    #[serde(default)]
    method: u64,
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json", default)]
    params: RawBytes,
    #[schemars(with = "LotusJson<Option<Cid>>")]
    #[serde(
        with = "crate::lotus_json",
        rename = "CID",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    cid: Option<Cid>,
}

impl HasLotusJson for Message {
    type LotusJson = MessageLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        let msg = Message::default();
        let cid = msg.cid();
        vec![(
            json!({
                "From": "f00",
                "GasFeeCap": "0",
                "GasLimit": 0,
                "GasPremium": "0",
                "Method": 0,
                "Nonce": 0,
                "Params": null,
                "To": "f00",
                "Value": "0",
                "Version": 0,
                "CID": { "/": cid.to_string() },
            }),
            msg,
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let cid = Some(self.cid());
        // The only lossless way to take a `Message` apart by value from outside its module.
        let Message_v4 {
            version,
            from,
            to,
            sequence,
            value,
            method_num,
            params,
            gas_limit,
            gas_fee_cap,
            gas_premium,
        } = self.into();
        Self::LotusJson {
            version,
            to: to.into(),
            from: from.into(),
            nonce: sequence,
            value: value.into(),
            gas_limit,
            gas_fee_cap: gas_fee_cap.into(),
            gas_premium: gas_premium.into(),
            method: method_num,
            params,
            cid,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            version,
            to,
            from,
            nonce,
            value,
            gas_limit,
            gas_fee_cap,
            gas_premium,
            method,
            params,
            cid: _,
        } = lotus_json;
        Message::builder()
            .version(version)
            .from(from)
            .to(to)
            .sequence(nonce)
            .value(value)
            .method_num(method)
            .params(params)
            .gas_limit(gas_limit)
            .gas_fee_cap(gas_fee_cap)
            .gas_premium(gas_premium)
            .build()
    }
}
