// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
pub mod json {
    use crate::message::SignedMessage;
    use crate::shim::{crypto::Signature, message::Message};
    use cid::Cid;
    use fvm_ipld_encoding::Cbor;
    use serde::{ser, Deserialize, Deserializer, Serialize, Serializer};

    use crate::json::{message, signature};

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
            #[serde(default, rename = "CID", with = "crate::json::cid::opt")]
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
        use crate::utils::json::GoVecVisitor;
        use serde::ser::SerializeSeq;

        use super::*;

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
    use crate::message::{self, SignedMessage};
    use crate::shim::{
        address::Address,
        crypto::Signature,
        econ::TokenAmount,
        message::{Message, Message_v3},
    };
    use quickcheck_macros::quickcheck;
    use serde::{Deserialize, Serialize};
    use serde_json::{self, from_str, to_string};

    use super::json::{SignedMessageJson, SignedMessageJsonRef};
    use crate::json::message::json::{MessageJson, MessageJsonRef};

    #[quickcheck]
    fn signed_message_roundtrip(signed_message: SignedMessage) {
        let serialized = serde_json::to_string(&SignedMessageJsonRef(&signed_message)).unwrap();
        let parsed: SignedMessageJson = serde_json::from_str(&serialized).unwrap();
        // Skip delegated addresses for now
        if (signed_message.message.from.protocol() != crate::shim::address::Protocol::Delegated)
            && (signed_message.message.to.protocol() != crate::shim::address::Protocol::Delegated)
        {
            assert_eq!(signed_message, parsed.0)
        }
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
        let message: Message = Message_v3 {
            version: 10,
            from: Address::new_id(34).into(),
            to: Address::new_id(12).into(),
            sequence: 5,
            value: TokenAmount::from_atto(6).into(),
            method_num: 7,
            params: Default::default(),
            gas_limit: 8,
            gas_fee_cap: TokenAmount::from_atto(10).into(),
            gas_premium: TokenAmount::from_atto(9).into(),
        }
        .into();

        let signed = SignedMessage::new_unchecked(message.clone(), Signature::new_bls(vec![0, 1]));

        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct TestStruct {
            #[serde(with = "crate::json::message::json")]
            unsigned: Message,
            #[serde(with = "crate::json::signed_message::json")]
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
