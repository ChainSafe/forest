// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(feature = "proofs")]
use fvm::gas::Gas;
#[cfg(feature = "proofs")]
use fvm_shared::message::Message;

/// Semantic validation and validates the message has enough gas.
#[cfg(feature = "proofs")]
pub fn valid_for_block_inclusion(
    msg: &Message,
    min_gas: Gas,
    version: fil_types::NetworkVersion,
) -> Result<(), anyhow::Error> {
    use fil_types::{NetworkVersion, BLOCK_GAS_LIMIT, TOTAL_FILECOIN, ZERO_ADDRESS};
    use num_traits::Signed;
    if msg.version != 0 {
        anyhow::bail!("Message version: {} not supported", msg.version);
    }
    if msg.to == *ZERO_ADDRESS && version >= NetworkVersion::V7 {
        anyhow::bail!("invalid 'to' address");
    }
    if msg.value.is_negative() {
        anyhow::bail!("message value cannot be negative");
    }
    if msg.value > *TOTAL_FILECOIN {
        anyhow::bail!("message value cannot be greater than total FIL supply");
    }
    if msg.gas_fee_cap.is_negative() {
        anyhow::bail!("gas_fee_cap cannot be negative");
    }
    if msg.gas_premium.is_negative() {
        anyhow::bail!("gas_premium cannot be negative");
    }
    if msg.gas_premium > msg.gas_fee_cap {
        anyhow::bail!("gas_fee_cap less than gas_premium");
    }
    if msg.gas_limit > BLOCK_GAS_LIMIT {
        anyhow::bail!("gas_limit cannot be greater than block gas limit");
    }

    if Gas::new(msg.gas_limit) < min_gas {
        anyhow::bail!(
            "gas_limit {} cannot be less than cost {} of storing a message on chain",
            msg.gas_limit,
            min_gas
        );
    }

    Ok(())
}

#[cfg(feature = "json")]
pub mod json {
    use cid::Cid;
    use forest_json::address::json::AddressJson;
    use forest_json::bigint;
    use forest_vm::{Serialized, TokenAmount};
    use fvm_ipld_encoding::Cbor;
    use fvm_shared::message::Message;
    use serde::{de, ser};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing and deserializing a Message from JSON.
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
        version: i64,
        to: AddressJson,
        from: AddressJson,
        #[serde(rename = "Nonce")]
        sequence: u64,
        #[serde(with = "bigint::json")]
        value: TokenAmount,
        gas_limit: i64,
        #[serde(with = "bigint::json")]
        gas_fee_cap: TokenAmount,
        #[serde(with = "bigint::json")]
        gas_premium: TokenAmount,
        #[serde(rename = "Method")]
        method_num: u64,
        params: Option<String>,
        #[serde(default, rename = "CID", with = "forest_json::cid::opt")]
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
            value: m.value.clone(),
            gas_limit: m.gas_limit,
            gas_fee_cap: m.gas_fee_cap.clone(),
            gas_premium: m.gas_premium.clone(),
            method_num: m.method_num,
            params: Some(base64::encode(m.params.bytes())),
            cid: Some(m.cid().map_err(ser::Error::custom)?),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Message, D::Error>
    where
        D: Deserializer<'de>,
    {
        let m: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(Message {
            version: m.version,
            to: m.to.into(),
            from: m.from.into(),
            sequence: m.sequence,
            value: m.value,
            gas_limit: m.gas_limit,
            gas_fee_cap: m.gas_fee_cap,
            gas_premium: m.gas_premium,
            method_num: m.method_num,
            params: Serialized::new(
                base64::decode(&m.params.unwrap_or_else(|| "".to_string()))
                    .map_err(de::Error::custom)?,
            ),
        })
    }

    pub mod vec {
        use super::*;
        use forest_json_utils::GoVecVisitor;
        use serde::ser::SerializeSeq;

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
