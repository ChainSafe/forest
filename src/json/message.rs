// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use crate::message::Message as MessageTrait;
    use crate::shim::{address::Address, econ::TokenAmount, message::Message};
    use base64::{prelude::BASE64_STANDARD, Engine};
    use cid::Cid;
    use fvm_ipld_encoding::RawBytes;
    use fvm_shared3::message::Message as Message_v3;
    use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};

    use crate::json::address::json::AddressJson;

    /// Wrapper for serializing and de-serializing a Message from JSON.
    #[derive(Deserialize, Serialize, Debug)]
    #[serde(transparent)]
    pub struct MessageJson(#[serde(with = "self")] pub Message);

    /// Wrapper for serializing a Message reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct MessageJsonRef<'a>(#[serde(with = "self")] pub &'a Message);

    impl From<MessageJson> for Message {
        fn from(wrapper: MessageJson) -> Self {
            wrapper.0
        }
    }

    impl From<Message> for MessageJson {
        fn from(wrapper: Message) -> Self {
            MessageJson(wrapper)
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        version: u64,
        to: AddressJson,
        from: AddressJson,
        #[serde(rename = "Nonce")]
        sequence: u64,
        #[serde(with = "crate::json::token_amount::json")]
        value: TokenAmount,
        gas_limit: u64,
        #[serde(with = "crate::json::token_amount::json")]
        gas_fee_cap: TokenAmount,
        #[serde(with = "crate::json::token_amount::json")]
        gas_premium: TokenAmount,
        #[serde(rename = "Method")]
        method_num: u64,
        params: Option<String>,
        #[serde(default, rename = "CID", with = "crate::json::cid::opt")]
        cid: Option<Cid>,
    }

    pub fn serialize<S>(m: &Message, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            version: m.version,
            to: m.to.into(),
            from: m.from.into(),
            sequence: m.sequence,
            value: m.value(),
            gas_limit: m.gas_limit(),
            gas_fee_cap: m.gas_fee_cap(),
            gas_premium: m.gas_premium(),
            method_num: m.method_num,
            params: Some(BASE64_STANDARD.encode(m.params.bytes())),
            cid: Some(m.cid().map_err(ser::Error::custom)?),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Message, D::Error>
    where
        D: Deserializer<'de>,
    {
        let m: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(Message_v3 {
            version: m.version,
            to: Address::from(m.to).into(),
            from: Address::from(m.from).into(),
            sequence: m.sequence,
            value: m.value.into(),
            gas_limit: m.gas_limit,
            gas_fee_cap: m.gas_fee_cap.into(),
            gas_premium: m.gas_premium.into(),
            method_num: m.method_num,
            params: RawBytes::new(
                BASE64_STANDARD
                    .decode(m.params.unwrap_or_default())
                    .map_err(de::Error::custom)?,
            ),
        }
        .into())
    }

    pub mod vec {
        use crate::utils::json::GoVecVisitor;
        use serde::ser::SerializeSeq;

        use super::*;

        pub fn serialize<S>(m: &[Message], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut seq = serializer.serialize_seq(Some(m.len()))?;
            for e in m {
                seq.serialize_element(&MessageJsonRef(e))?;
            }
            seq.end()
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Message>, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(GoVecVisitor::<Message, MessageJson>::new())
        }
    }
}

#[cfg(test)]
pub mod tests {
    use crate::shim::message::Message;
    use quickcheck_macros::quickcheck;

    use super::json::{MessageJson, MessageJsonRef};

    #[quickcheck]
    fn message_roundtrip(message: Message) {
        let serialized = serde_json::to_string(&MessageJsonRef(&message)).unwrap();
        let parsed: MessageJson = serde_json::from_str(&serialized).unwrap();
        // Skip delegated addresses for now
        if (message.from.protocol() != crate::shim::address::Protocol::Delegated)
            && (message.to.protocol() != crate::shim::address::Protocol::Delegated)
        {
            assert_eq!(message, parsed.0)
        }
    }
}
