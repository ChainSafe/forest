// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::message::SignedMessage;
use crate::shim::{crypto::Signature, message::Message};
use ::cid::Cid;

use super::*;

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct SignedMessageLotusJson {
    message: LotusJson<Message>,
    signature: LotusJson<Signature>,
    #[serde(rename = "CID", skip_serializing_if = "LotusJson::is_none", default)]
    cid: LotusJson<Option<Cid>>,
}

impl SignedMessageLotusJson {
    pub fn with_cid(mut self, cid: Cid) -> Self {
        self.cid = LotusJson(Some(cid));
        self
    }
}

impl HasLotusJson for SignedMessage {
    type LotusJson = SignedMessageLotusJson;

    #[cfg(test)]
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
                    "Params": null,
                    "To": "f00",
                    "Value": "0",
                    "Version": 0,
                    "CID": {
                        "/": "bafy2bzaced3xdk2uf6azekyxgcttujvy3fzyeqmibtpjf2fxcpfdx2zcx4s3g"
                    },
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

    fn into_lotus_json(self) -> Self::LotusJson {
        let Self { message, signature } = self;
        Self::LotusJson {
            message: message.into(),
            signature: signature.into(),
            cid: None.into(), // See notes on Message
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            message,
            signature,
            cid: _ignored, // See notes on Message
        } = lotus_json;
        Self {
            message: message.into_inner(),
            signature: signature.into_inner(),
        }
    }
}
