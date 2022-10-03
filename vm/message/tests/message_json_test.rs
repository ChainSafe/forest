// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "json")]

use forest_crypto::Signer;
use forest_message::message;
use forest_message::message::json::{MessageJson, MessageJsonRef};
use forest_message::signed_message::{
    self,
    json::{SignedMessageJson, SignedMessageJsonRef},
    SignedMessage,
};
use fvm_ipld_encoding::RawBytes;
use fvm_shared::address::Address;
use fvm_shared::crypto::signature::Signature;
use fvm_shared::message::Message;
use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string};

#[test]
fn unsigned_symmetric_json() {
    let message_json = r#"{"Version":9,"To":"f01234","From":"f01234","Nonce":42,"Value":"0","GasLimit":1,"GasFeeCap":"2","GasPremium":"9","Method":1,"Params":"Ynl0ZSBhcnJheQ==","CID":{"/":"bafy2bzacea5z7ywqogtuxykqcis76lrhl4fl27bg63firldlry5ml6bbahoy6"}}"#;

    // Deserialize
    let MessageJson(cid_d) = from_str(message_json).unwrap();

    // Serialize
    let ser_cid = to_string(&MessageJsonRef(&cid_d)).unwrap();
    assert_eq!(ser_cid, message_json);
}

#[test]
fn signed_symmetric_json() {
    let message_json = r#"{"Message":{"Version":9,"To":"f01234","From":"f01234","Nonce":42,"Value":"0","GasLimit":1,"GasFeeCap":"2","GasPremium":"9","Method":1,"Params":"Ynl0ZSBhcnJheQ==","CID":{"/":"bafy2bzacea5z7ywqogtuxykqcis76lrhl4fl27bg63firldlry5ml6bbahoy6"}},"Signature":{"Type":2,"Data":"Ynl0ZSBhcnJheQ=="},"CID":{"/":"bafy2bzacea5z7ywqogtuxykqcis76lrhl4fl27bg63firldlry5ml6bbahoy6"}}"#;

    // Deserialize
    let SignedMessageJson(cid_d) = from_str(message_json).unwrap();

    // Serialize
    let ser_cid = to_string(&SignedMessageJsonRef(&cid_d)).unwrap();
    assert_eq!(ser_cid, message_json);
}

#[test]
fn message_json_annotations() {
    let message = Message {
        version: 10,
        from: Address::new_id(34),
        to: Address::new_id(12),
        sequence: 5,
        value: 6.into(),
        method_num: 7,
        params: RawBytes::default(),
        gas_limit: 8,
        gas_fee_cap: 10.into(),
        gas_premium: 9.into(),
    };

    struct DummySigner;
    impl Signer for DummySigner {
        fn sign_bytes(&self, _: &[u8], _: &Address) -> Result<Signature, anyhow::Error> {
            Ok(Signature::new_bls(vec![0u8, 1u8]))
        }
    }
    let signed = SignedMessage::new(message.clone(), &DummySigner).unwrap();

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestStruct {
        #[serde(with = "message::json")]
        unsigned: Message,
        #[serde(with = "signed_message::json")]
        signed: SignedMessage,
    }
    let test_json = r#"
        {
            "unsigned": {
                "Version": 10,
                "To": "f012",
                "From": "f034",
                "Nonce": 5,
                "Value": "6",
                "GasPremium": "9",
                "GasFeeCap": "10",
                "GasLimit": 8,
                "Method": 7,
                "Params": ""
            },
            "signed": {
                "Message": {
                    "Version": 10,
                    "To": "f012",
                    "From": "f034",
                    "Nonce": 5,
                    "Value": "6",
                    "GasPremium": "9",
                    "GasFeeCap": "10",
                    "GasLimit": 8,
                    "Method": 7,
                    "Params": ""
                },
                "Signature": {
                    "Type": 2,
                    "Data": "AAE="
                }
            }
        }
        "#;
    let expected = TestStruct {
        unsigned: message,
        signed,
    };
    assert_eq!(from_str::<TestStruct>(test_json).unwrap(), expected);
}
