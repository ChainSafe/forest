// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use crate::shim::{address::Address, econ::TokenAmount, message::Message};
use ::cid::Cid;
use fvm_ipld_encoding::RawBytes;

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct MessageLotusJson {
    version: LotusJson<u64>,
    to: LotusJson<Address>,
    from: LotusJson<Address>,
    nonce: LotusJson<u64>,
    value: LotusJson<TokenAmount>,
    gas_limit: LotusJson<u64>,
    gas_fee_cap: LotusJson<TokenAmount>,
    gas_premium: LotusJson<TokenAmount>,
    method: LotusJson<u64>,
    #[serde(skip_serializing_if = "LotusJson::is_none", default)]
    params: LotusJson<Option<RawBytes>>,
    // This is a bit of a hack - `Message`s don't really store their CID, but they're
    // serialized with it.
    // However, getting a message's CID is fallible...
    // So we keep this as an `Option`, and ignore it if it fails.
    // We also ignore it when serializing from json.
    // I wouldn't be surprised if this causes issues with arbitrary tests
    #[serde(rename = "CID", skip_serializing_if = "LotusJson::is_none", default)]
    cid: LotusJson<Option<Cid>>,
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
            version: version.into(),
            to: to.into(),
            from: from.into(),
            nonce: sequence.into(),
            value: value.into(),
            gas_limit: gas_limit.into(),
            gas_fee_cap: gas_fee_cap.into(),
            gas_premium: gas_premium.into(),
            method: method_num.into(),
            params: Some(params).into(),
            cid: cid.into(),
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
            version: version.into_inner(),
            from: from.into_inner(),
            to: to.into_inner(),
            sequence: nonce.into_inner(),
            value: value.into_inner(),
            method_num: method.into_inner(),
            params: params.into_inner().unwrap_or_default(),
            gas_limit: gas_limit.into_inner(),
            gas_fee_cap: gas_fee_cap.into_inner(),
            gas_premium: gas_premium.into_inner(),
        }
    }
}
