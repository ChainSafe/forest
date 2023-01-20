// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
pub mod json {
    use crate::{message, signature};
    use cid::Cid;
    use forest_message::SignedMessage;
    use fvm_ipld_encoding::Cbor;
    use fvm_shared::crypto::signature::Signature;
    use fvm_shared::message::Message;
    use serde::{ser, Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing and de-serializing a `SignedMessage` from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct SignedMessageJson(#[serde(with = "self")] pub SignedMessage);

    /// Wrapper for serializing a `SignedMessage` reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct SignedMessageJsonRef<'a>(#[serde(with = "self")] pub &'a SignedMessage);

    impl From<SignedMessageJson> for SignedMessage {
        fn from(wrapper: SignedMessageJson) -> Self {
            wrapper.0
        }
    }

    impl From<SignedMessage> for SignedMessageJson {
        fn from(msg: SignedMessage) -> Self {
            SignedMessageJson(msg)
        }
    }

    pub fn serialize<S>(m: &SignedMessage, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct SignedMessageSer<'a> {
            #[serde(with = "message::json")]
            message: &'a Message,
            #[serde(with = "signature::json")]
            signature: &'a Signature,
            #[serde(default, rename = "CID", with = "crate::cid::opt")]
            cid: Option<Cid>,
        }
        SignedMessageSer {
            message: &m.message,
            signature: &m.signature,
            cid: Some(m.cid().map_err(ser::Error::custom)?),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SignedMessage, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct SignedMessageDe {
            #[serde(with = "message::json")]
            message: Message,
            #[serde(with = "signature::json")]
            signature: Signature,
        }
        let SignedMessageDe { message, signature } = Deserialize::deserialize(deserializer)?;
        Ok(SignedMessage { message, signature })
    }

    pub mod vec {
        use super::*;
        use forest_utils::json::GoVecVisitor;
        use serde::ser::SerializeSeq;

        pub fn serialize<S>(m: &[SignedMessage], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut seq = serializer.serialize_seq(Some(m.len()))?;
            for e in m {
                seq.serialize_element(&SignedMessageJsonRef(e))?;
            }
            seq.end()
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<SignedMessage>, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(GoVecVisitor::<SignedMessage, SignedMessageJson>::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::json::{SignedMessageJson, SignedMessageJsonRef};
    use crate::message;
    use crate::message::json::{MessageJson, MessageJsonRef};
    use forest_message::{self, SignedMessage};
    use fvm_ipld_encoding::RawBytes;
    use fvm_shared::address::Address;
    use fvm_shared::crypto::signature::Signature;
    use fvm_shared::econ::TokenAmount;
    use fvm_shared::message::Message;
    use quickcheck_macros::quickcheck;
    use serde::{Deserialize, Serialize};
    use serde_json;
    use serde_json::{from_str, to_string};

    #[derive(Clone, Debug, PartialEq)]
    struct SignedMessageWrapper(SignedMessage);

    impl quickcheck::Arbitrary for SignedMessageWrapper {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            SignedMessageWrapper(SignedMessage::new_unchecked(
                crate::message::tests::MessageWrapper::arbitrary(g).message,
                Signature::new_secp256k1(vec![0]),
            ))
        }
    }

    #[quickcheck]
    fn signed_message_roundtrip(message: SignedMessageWrapper) {
        let serialized = serde_json::to_string(&SignedMessageJsonRef(&message.0)).unwrap();
        let parsed: SignedMessageJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(message.0, parsed.0);
    }

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
            value: TokenAmount::from_atto(6),
            method_num: 7,
            params: RawBytes::default(),
            gas_limit: 8,
            gas_fee_cap: TokenAmount::from_atto(10),
            gas_premium: TokenAmount::from_atto(9),
        };

        let signed = SignedMessage::new_unchecked(message.clone(), Signature::new_bls(vec![0, 1]));

        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct TestStruct {
            #[serde(with = "message::json")]
            unsigned: Message,
            #[serde(with = "crate::signed_message::json")]
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
}
