// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::message::SignedMessage;

use super::*;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SignedMessageLotusJson {
    message: MessageLotusJson,
    signature: SignatureLotusJson,
    #[serde(rename = "CID", skip_serializing_if = "Option::is_none")]
    cid: Option<CidLotusJson>,
}

impl HasLotusJson for SignedMessage {
    type LotusJson = SignedMessageLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Message": {
                    "From": "f00",
                    "GasFeeCap": "0",
                    "GasLimit": 0,
                    "GasPremium": "0",
                    "Method": 0,
                    "Nonce": 0,
                    "Params": "",
                    "To": "f00",
                    "Value": "0",
                    "Version": 0
                },
                "Signature": {"Type": 2, "Data": "aGVsbG8gd29ybGQh"}
            }),
            SignedMessage {
                message: crate::shim::message::Message::default(),
                signature: crate::shim::crypto::Signature {
                    sig_type: crate::shim::crypto::SignatureType::Bls,
                    bytes: Vec::from_iter(*b"hello world!"),
                },
            },
        )]
    }
}

impl From<SignedMessage> for SignedMessageLotusJson {
    fn from(value: SignedMessage) -> Self {
        let SignedMessage { message, signature } = value;
        Self {
            message: message.into(),
            signature: signature.into(),
            cid: None, // BUG?(aatifsyed)
        }
    }
}

impl From<SignedMessageLotusJson> for SignedMessage {
    fn from(value: SignedMessageLotusJson) -> Self {
        let SignedMessageLotusJson {
            message,
            signature,
            cid: _ignored, // BUG?(aatifsyed)
        } = value;
        Self {
            message: message.into(),
            signature: signature.into(),
        }
    }
}
