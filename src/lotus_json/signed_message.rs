// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::message::SignedMessage;
use crate::shim::{crypto::Signature, message::Message};
use ::cid::Cid;

use super::*;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "SignedMessage")]
pub struct SignedMessageLotusJson {
    #[schemars(with = "LotusJson<Message>")]
    #[serde(with = "crate::lotus_json")]
    message: Message,
    #[schemars(with = "LotusJson<Signature>")]
    #[serde(with = "crate::lotus_json")]
    signature: Signature,
    #[schemars(with = "LotusJson<Option<Cid>>")]
    #[serde(
        with = "crate::lotus_json",
        rename = "CID",
        skip_serializing_if = "Option::is_none",
        default
    )]
    cid: Option<Cid>,
}

impl SignedMessageLotusJson {
    pub fn with_cid(mut self, cid: Cid) -> Self {
        self.cid = Some(cid);
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
                },
                "Signature": {"Type": 2, "Data": "aGVsbG8gd29ybGQh"},
                "CID": {
                    "/": "bafy2bzaced3xdk2uf6azekyxgcttujvy3fzyeqmibtpjf2fxcpfdx2zcx4s3g"
                },
            }),
            SignedMessage {
                message: Message::default(),
                signature: Signature {
                    sig_type: crate::shim::crypto::SignatureType::Bls,
                    bytes: Vec::from_iter(*b"hello world!"),
                },
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let cid = Some(self.cid());
        let Self { message, signature } = self;
        Self::LotusJson {
            message,
            signature,
            cid,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            message,
            signature,
            cid: _ignored, // See notes on Message
        } = lotus_json;
        Self { message, signature }
    }
}
