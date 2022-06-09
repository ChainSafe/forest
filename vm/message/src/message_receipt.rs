// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use vm::ExitCode;

use fvm_ipld_encoding::RawBytes;
use fvm_shared::receipt::Receipt;

/// Result of a state transition from a message
pub type MessageReceipt = Receipt;

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing and deserializing a SignedMessage from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct MessageReceiptJson(#[serde(with = "self")] pub MessageReceipt);

    /// Wrapper for serializing a SignedMessage reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct MessageReceiptJsonRef<'a>(#[serde(with = "self")] pub &'a MessageReceipt);

    impl From<MessageReceiptJson> for MessageReceipt {
        fn from(wrapper: MessageReceiptJson) -> Self {
            wrapper.0
        }
    }

    impl From<MessageReceipt> for MessageReceiptJson {
        fn from(wrapper: MessageReceipt) -> Self {
            MessageReceiptJson(wrapper)
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        exit_code: u64,
        #[serde(rename = "Return")]
        return_data: String,
        gas_used: i64,
    }

    pub fn serialize<S>(m: &MessageReceipt, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            exit_code: m.exit_code.value() as u64,
            return_data: base64::encode(m.return_data.bytes()),
            gas_used: m.gas_used,
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<MessageReceipt, D::Error>
    where
        D: Deserializer<'de>,
    {
        let JsonHelper {
            exit_code,
            return_data,
            gas_used,
        } = Deserialize::deserialize(deserializer)?;
        Ok(MessageReceipt {
            exit_code: ExitCode::new(exit_code as u32),
            return_data: RawBytes::new(base64::decode(&return_data).map_err(de::Error::custom)?),
            gas_used,
        })
    }
    pub mod vec {
        use super::*;
        use forest_json_utils::GoVecVisitor;
        use serde::ser::SerializeSeq;

        pub fn serialize<S>(m: &[MessageReceipt], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut seq = serializer.serialize_seq(Some(m.len()))?;
            for e in m {
                seq.serialize_element(&MessageReceiptJsonRef(e))?;
            }
            seq.end()
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<MessageReceipt>, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(GoVecVisitor::<MessageReceipt, MessageReceiptJson>::new())
        }
    }

    pub mod opt {
        use super::*;

        pub fn serialize<S>(v: &Option<MessageReceipt>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            v.as_ref().map(MessageReceiptJsonRef).serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<MessageReceipt>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s: Option<MessageReceipt> = Deserialize::deserialize(deserializer)?;
            Ok(s)
        }
    }
}
