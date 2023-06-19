// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::executor::{Receipt, Receipt_v3};

pub mod json {
    use crate::shim::error::ExitCode;
    use base64::{prelude::BASE64_STANDARD, Engine};
    use fvm_ipld_encoding3::RawBytes;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    use super::*;

    /// Wrapper for serializing and de-serializing a `SignedMessage` from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct ReceiptJson(#[serde(with = "self")] pub Receipt);

    /// Wrapper for serializing a `SignedMessage` reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct ReceiptJsonRef<'a>(#[serde(with = "self")] pub &'a Receipt);

    impl From<ReceiptJson> for Receipt {
        fn from(wrapper: ReceiptJson) -> Self {
            wrapper.0
        }
    }

    impl From<Receipt> for ReceiptJson {
        fn from(wrapper: Receipt) -> Self {
            ReceiptJson(wrapper)
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        exit_code: u64,
        #[serde(rename = "Return")]
        return_data: String,
        gas_used: u64,
    }

    pub fn serialize<S>(m: &Receipt, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            exit_code: m.exit_code().value() as u64,
            return_data: BASE64_STANDARD.encode(m.return_data().bytes()),
            gas_used: m.gas_used(),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Receipt, D::Error>
    where
        D: Deserializer<'de>,
    {
        let JsonHelper {
            exit_code,
            return_data,
            gas_used,
        } = Deserialize::deserialize(deserializer)?;
        Ok(Receipt_v3 {
            exit_code: ExitCode::from(exit_code as u32).into(),
            return_data: RawBytes::new(
                BASE64_STANDARD
                    .decode(return_data)
                    .map_err(de::Error::custom)?,
            ),
            gas_used,
            events_root: None,
        }
        .into())
    }
    pub mod vec {
        use crate::utils::json::GoVecVisitor;
        use serde::ser::SerializeSeq;

        use super::*;

        pub fn serialize<S>(m: &[Receipt], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut seq = serializer.serialize_seq(Some(m.len()))?;
            for e in m {
                seq.serialize_element(&ReceiptJsonRef(e))?;
            }
            seq.end()
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Receipt>, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(GoVecVisitor::<Receipt, ReceiptJson>::new())
        }
    }

    pub mod opt {
        use super::*;

        pub fn serialize<S>(v: &Option<Receipt>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            v.as_ref().map(ReceiptJsonRef).serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Receipt>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s: Option<Receipt> = Deserialize::deserialize(deserializer)?;
            Ok(s)
        }
    }
}

#[cfg(test)]
#[derive(Clone, Debug)]
struct MessageReceiptWrapper {
    message_receipt: Receipt,
}

#[cfg(test)]
impl quickcheck::Arbitrary for MessageReceiptWrapper {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let message_receipt = Receipt_v3 {
            exit_code: crate::shim::error::ExitCode::from(u32::arbitrary(g)).into(),
            return_data: fvm_ipld_encoding3::RawBytes::new(Vec::arbitrary(g)),
            gas_used: u64::arbitrary(g),
            events_root: None,
        };
        MessageReceiptWrapper {
            message_receipt: message_receipt.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use quickcheck_macros::quickcheck;
    use serde_json;

    use super::{
        json::{ReceiptJson, ReceiptJsonRef},
        *,
    };

    #[quickcheck]
    fn message_receipt_roundtrip(message_receipt: MessageReceiptWrapper) {
        let serialized =
            serde_json::to_string(&ReceiptJsonRef(&message_receipt.message_receipt)).unwrap();
        let parsed: ReceiptJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(message_receipt.message_receipt, parsed.0);
    }
}
