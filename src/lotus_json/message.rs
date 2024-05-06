// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use crate::shim::{address::Address, econ::TokenAmount, message::Message};
use ::cid::Cid;
use fvm_ipld_encoding::RawBytes;

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct MessageLotusJson {
    version: u64,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    to: Address,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    from: Address,
    nonce: u64,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    value: TokenAmount,
    gas_limit: u64,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    gas_fee_cap: TokenAmount,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    gas_premium: TokenAmount,
    method: u64,
    #[schemars(with = "LotusJson<Option<RawBytes>>")]
    #[serde(
        with = "crate::lotus_json",
        skip_serializing_if = "Option::is_none",
        default
    )]
    params: Option<RawBytes>,
    // This is a bit of a hack - `Message`s don't really store their CID, but they're
    // serialized with it.
    // However, getting a message's CID is fallible...
    // So we keep this as an `Option`, and ignore it if it fails.
    // We also ignore it when serializing from json.
    // I wouldn't be surprised if this causes issues with arbitrary tests
    #[schemars(with = "LotusJson<Option<Cid>>")]
    #[serde(
        with = "crate::lotus_json",
        rename = "CID",
        skip_serializing_if = "Option::is_none",
        default
    )]
    cid: Option<Cid>,
}

impl HasLotusJson for Message {
    type LotusJson = MessageLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
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
                "CID": {
                    "/": "bafy2bzaced3xdk2uf6azekyxgcttujvy3fzyeqmibtpjf2fxcpfdx2zcx4s3g"
                }
            }),
            Message::default(),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let cid = self.cid().ok();
        let Self {
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
        } = self;
        Self::LotusJson {
            version,
            to,
            from,
            nonce: sequence,
            value,
            gas_limit,
            gas_fee_cap,
            gas_premium,
            method: method_num,
            params: Some(params),
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
            cid: _ignored,
        } = lotus_json;
        Self {
            version,
            from,
            to,
            sequence: nonce,
            value,
            method_num: method,
            params: params.unwrap_or_default(),
            gas_limit,
            gas_fee_cap,
            gas_premium,
        }
    }
}
