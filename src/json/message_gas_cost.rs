// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::interpreter::MessageGasCost;

pub mod json {
    use cid::Cid;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::*;

    /// Wrapper for serializing and de-serializing an `MessageGasCost` from JSON.
    #[derive(Deserialize, Serialize, Debug)]
    #[serde(transparent)]
    pub struct MessageGasCostJson(#[serde(with = "self")] pub MessageGasCost);

    impl From<MessageGasCostJson> for MessageGasCost {
        fn from(wrapper: MessageGasCostJson) -> Self {
            wrapper.0
        }
    }

    impl From<MessageGasCost> for MessageGasCostJson {
        fn from(ir: MessageGasCost) -> Self {
            MessageGasCostJson(ir)
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        #[serde(default, with = "crate::json::cid")]
        pub message: Cid,
        pub gas_used: u64,
    }

    pub fn serialize<S>(gc: &MessageGasCost, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            message: gc.message,
            gas_used: gc.gas_used,
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<MessageGasCost, D::Error>
    where
        D: Deserializer<'de>,
    {
        let gc: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(MessageGasCost {
            message: gc.message,
            gas_used: gc.gas_used,
        })
    }
}

#[cfg(test)]
pub mod tests {
    // todo!
}
