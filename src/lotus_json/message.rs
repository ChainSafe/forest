// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

use crate::shim::message::Message;

// TODO(aatifsyed): derive
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageLotusJson {
    version: u64,
    to: AddressLotusJson,
    from: AddressLotusJson,
    nonce: u64,
    value: TokenAmountLotusJson,
    gas_limit: u64,
    gas_fee_cap: TokenAmountLotusJson,
    gas_premium: TokenAmountLotusJson,
    method: u64,
    params: Option<RawBytesLotusJson>,
    #[serde(rename = "CID", skip_serializing_if = "Option::is_none")]
    cid: Option<CidLotusJson>,
}

impl HasLotusJson for Message {
    type LotusJson = MessageLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "From": "f00",
                "GasFeeCap": "0",
                "GasLimit": 0, // BUG?(aatifsyed)
                "GasPremium": "0",
                "Method": 0,
                "Nonce": 0,
                "Params": "", // BUG?(aatifsyed)
                "To": "f00",
                "Value": "0",
                "Version": 0
            }),
            Message::default(),
        )]
    }
}

// TODO(aatifsyed): derive
impl From<MessageLotusJson> for Message {
    fn from(value: MessageLotusJson) -> Self {
        let MessageLotusJson {
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
            cid: _ignored, // TODO(aatifsyed): is this an error?
        } = value;
        Self {
            version,
            from: from.into(),
            to: to.into(),
            sequence: nonce,
            value: value.into(),
            method_num: method,
            params: params.map(Into::into).unwrap_or_default(),
            gas_limit,
            gas_fee_cap: gas_fee_cap.into(),
            gas_premium: gas_premium.into(),
        }
    }
}

impl From<Message> for MessageLotusJson {
    fn from(value: Message) -> Self {
        let Message {
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
        } = value;
        Self {
            version,
            to: to.into(),
            from: from.into(),
            nonce: sequence,
            value: value.into(),
            gas_limit,
            gas_fee_cap: gas_fee_cap.into(),
            gas_premium: gas_premium.into(),
            method: method_num,
            params: Some(params.into()),
            cid: None, // TODO(aatifsyed): is this an error?
        }
    }
}
