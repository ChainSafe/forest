// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Message;
use address::Address;
use derive_builder::Builder;
use encoding::Cbor;
use num_bigint::bigint_ser::{BigIntDe, BigIntSer};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::{MethodNum, Serialized, TokenAmount};

/// Default Unsigned VM message type which includes all data needed for a state transition
///
/// Usage:
/// ```
/// use forest_message::{UnsignedMessage, Message};
/// use vm::{TokenAmount, Serialized, MethodNum};
/// use address::Address;
///
/// // Use the builder pattern to generate a message
/// let message = UnsignedMessage::builder()
///     .to(Address::new_id(0))
///     .from(Address::new_id(1))
///     .sequence(0) // optional
///     .value(TokenAmount::from(0u8)) // optional
///     .method_num(MethodNum::default()) // optional
///     .params(Serialized::default()) // optional
///     .gas_limit(0) // optional
///     .version(0) // optional
///     .build()
///     .unwrap();
///
/// // Commands can be chained, or built seperately
/// let mut message_builder = UnsignedMessage::builder();
/// message_builder.sequence(1);
/// message_builder.from(Address::new_id(0));
/// message_builder.to(Address::new_id(1));
/// let msg = message_builder.build().unwrap();
/// assert_eq!(msg.sequence(), 1);
/// ```
#[derive(PartialEq, Clone, Debug, Builder, Hash, Eq)]
#[builder(name = "MessageBuilder")]
pub struct UnsignedMessage {
    #[builder(default)]
    version: i64,
    from: Address,
    to: Address,
    #[builder(default)]
    sequence: u64,
    #[builder(default)]
    value: TokenAmount,
    #[builder(default)]
    method_num: MethodNum,
    #[builder(default)]
    params: Serialized,
    #[builder(default)]
    gas_limit: i64,
    #[builder(default)]
    gas_fee_cap: TokenAmount,
    #[builder(default)]
    gas_premium: TokenAmount,
}

impl UnsignedMessage {
    pub fn builder() -> MessageBuilder {
        MessageBuilder::default()
    }
}

impl Serialize for UnsignedMessage {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.version,
            &self.to,
            &self.from,
            &self.sequence,
            BigIntSer(&self.value),
            &self.gas_limit,
            BigIntSer(&self.gas_fee_cap),
            BigIntSer(&self.gas_premium),
            &self.method_num,
            &self.params,
        )
            .serialize(s)
    }
}

impl<'de> Deserialize<'de> for UnsignedMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (
            version,
            to,
            from,
            sequence,
            BigIntDe(value),
            gas_limit,
            BigIntDe(gas_fee_cap),
            BigIntDe(gas_premium),
            method_num,
            params,
        ) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            version,
            to,
            from,
            sequence,
            value,
            gas_limit,
            gas_fee_cap,
            gas_premium,
            method_num,
            params,
        })
    }
}

impl Message for UnsignedMessage {
    fn from(&self) -> &Address {
        &self.from
    }
    fn to(&self) -> &Address {
        &self.to
    }
    fn sequence(&self) -> u64 {
        self.sequence
    }
    fn value(&self) -> &TokenAmount {
        &self.value
    }
    fn method_num(&self) -> MethodNum {
        self.method_num
    }
    fn params(&self) -> &Serialized {
        &self.params
    }
    fn set_sequence(&mut self, new_sequence: u64) {
        self.sequence = new_sequence
    }
    fn gas_limit(&self) -> i64 {
        self.gas_limit
    }
    fn gas_fee_cap(&self) -> &TokenAmount {
        &self.gas_fee_cap
    }
    fn gas_premium(&self) -> &TokenAmount {
        &self.gas_premium
    }
    fn set_gas_limit(&mut self, token_amount: i64) {
        self.gas_limit = token_amount
    }
    fn set_gas_fee_cap(&mut self, cap: TokenAmount) {
        self.gas_fee_cap = cap;
    }
    fn set_gas_premium(&mut self, prem: TokenAmount) {
        self.gas_premium = prem;
    }
    fn required_funds(&self) -> TokenAmount {
        let total: TokenAmount = self.gas_fee_cap() * self.gas_limit();
        total + self.value()
    }
}

impl Cbor for UnsignedMessage {}

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use serde::de;

    /// Wrapper for serializing and deserializing a UnsignedMessage from JSON.
    #[derive(Deserialize, Serialize, Debug)]
    #[serde(transparent)]
    pub struct UnsignedMessageJson(#[serde(with = "self")] pub UnsignedMessage);

    /// Wrapper for serializing a UnsignedMessage reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct UnsignedMessageJsonRef<'a>(#[serde(with = "self")] pub &'a UnsignedMessage);

    impl From<UnsignedMessageJson> for UnsignedMessage {
        fn from(wrapper: UnsignedMessageJson) -> Self {
            wrapper.0
        }
    }

    impl From<UnsignedMessage> for UnsignedMessageJson {
        fn from(wrapper: UnsignedMessage) -> Self {
            UnsignedMessageJson(wrapper)
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        version: i64,
        to: String,
        from: String,
        #[serde(rename = "Nonce")]
        sequence: u64,
        value: String,
        gas_limit: i64,
        gas_fee_cap: String,
        gas_premium: String,
        #[serde(rename = "Method")]
        method_num: u64,
        params: Option<String>,
    }

    pub fn serialize<S>(m: &UnsignedMessage, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            version: m.version,
            to: m.to.to_string(),
            from: m.from.to_string(),
            sequence: m.sequence,
            value: m.value.to_string(),
            gas_limit: m.gas_limit,
            gas_fee_cap: m.gas_fee_cap.to_string(),
            gas_premium: m.gas_premium.to_string(),
            method_num: m.method_num,
            params: Some(base64::encode(m.params.bytes())),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<UnsignedMessage, D::Error>
    where
        D: Deserializer<'de>,
    {
        let m: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(UnsignedMessage {
            version: m.version,
            to: m.to.parse().map_err(de::Error::custom)?,
            from: m.from.parse().map_err(de::Error::custom)?,
            sequence: m.sequence,
            value: m.value.parse().map_err(de::Error::custom)?,
            gas_limit: m.gas_limit,
            gas_fee_cap: m.gas_fee_cap.parse().map_err(de::Error::custom)?,
            gas_premium: m.gas_premium.parse().map_err(de::Error::custom)?,
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

        pub fn serialize<S>(m: &[UnsignedMessage], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut seq = serializer.serialize_seq(Some(m.len()))?;
            for e in m {
                seq.serialize_element(&UnsignedMessageJsonRef(e))?;
            }
            seq.end()
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<UnsignedMessage>, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer
                .deserialize_any(GoVecVisitor::<UnsignedMessage, UnsignedMessageJson>::new())
        }
    }
}
