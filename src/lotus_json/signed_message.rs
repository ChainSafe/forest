// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::message::SignedMessage;
use crate::shim::{crypto::Signature, message::Message};
use ::cid::Cid;

use super::*;

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
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
        let cid = self.cid().ok();
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

#[test]
fn serialized_message_should_have_cid_at_top_level() {
    use crate::shim::address::Address;
    use crate::shim::crypto::SignatureType;
    use crate::shim::econ::TokenAmount;
    use fvm_ipld_encoding::RawBytes;
    use pretty_assertions::assert_eq;

    let signed_message = SignedMessage {
        message: Message {
            version: 0,
            from: Address::from_str("f1crmjzblza7nvhxbpvy2gps7oobypoqbn6ubttwa").unwrap(),
            to: Address::from_str("f1rxzkma2bo5jf5ab3mol7letujvvbf5xij7vngca").unwrap(),
            sequence: 6,
            value: TokenAmount::from_atto(500),
            method_num: 0,
            params: RawBytes::default(),
            gas_limit: 1518203,
            gas_fee_cap: TokenAmount::from_atto(100802),
            gas_premium: TokenAmount::from_atto(99748),
        },
        signature: Signature {
            sig_type: SignatureType::Secp256k1,
            bytes: vec![
                252, 15, 52, 235, 10, 182, 136, 84, 209, 139, 249, 129, 186, 28, 209, 130, 46, 148,
                79, 22, 238, 28, 221, 195, 218, 65, 127, 141, 226, 176, 85, 253, 73, 150, 141, 189,
                234, 16, 18, 139, 105, 12, 142, 118, 9, 194, 19, 4, 41, 114, 121, 96, 22, 138, 4,
                153, 81, 29, 96, 234, 117, 107, 42, 43, 0,
            ],
        },
    };

    let json = json!({
      "Message": {
        "Version": 0,
        "To": "f1rxzkma2bo5jf5ab3mol7letujvvbf5xij7vngca",
        "From": "f1crmjzblza7nvhxbpvy2gps7oobypoqbn6ubttwa",
        "Nonce": 6,
        "Value": "500",
        "GasLimit": 1518203,
        "GasFeeCap": "100802",
        "GasPremium": "99748",
        "Method": 0,
        "Params": null
      },
      "Signature": {
        "Type": 1,
        "Data": "/A806wq2iFTRi/mBuhzRgi6UTxbuHN3D2kF/jeKwVf1Jlo296hASi2kMjnYJwhMEKXJ5YBaKBJlRHWDqdWsqKwA="
      },
      "CID": {
        "/": "bafy2bzacebcd6s2tlyucpu5dkhxjubk5grlrhwacy2mnibojxs6m5jpiv6lic"
      }
    });

    assert_eq!(
        serde_json::to_value(signed_message.into_lotus_json()).unwrap(),
        json
    );
}
