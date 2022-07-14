// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Message;
use derive_builder::Builder;
use encoding::Cbor;
use forest_address::Address;
use forest_vm::{MethodNum, Serialized, TokenAmount};
#[cfg(feature = "proofs")]
use fvm::gas::Gas;
use fvm_shared::bigint::bigint_ser::{BigIntDe, BigIntSer};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Default Unsigned VM message type which includes all data needed for a state transition
///
/// Usage:
/// ```
/// use forest_message::{UnsignedMessage, Message};
/// use forest_vm::{TokenAmount, Serialized, MethodNum};
/// use forest_address::Address;
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
// /// Commands can be chained, or built separately
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
    pub version: i64,
    pub from: Address,
    pub to: Address,
    #[builder(default)]
    pub sequence: u64,
    #[builder(default)]
    pub value: TokenAmount,
    #[builder(default)]
    pub method_num: MethodNum,
    #[builder(default)]
    pub params: Serialized,
    #[builder(default)]
    pub gas_limit: i64,
    #[builder(default)]
    pub gas_fee_cap: TokenAmount,
    #[builder(default)]
    pub gas_premium: TokenAmount,
}

impl From<&UnsignedMessage> for fvm_shared::message::Message {
    fn from(msg: &UnsignedMessage) -> Self {
        let UnsignedMessage {
            version,
            from,
            to,
            sequence,
            value,
            method_num,
            params,
            gas_limit,
            gas_fee_cap,
            gas_premium,
        } = msg.clone();
        fvm_shared::message::Message {
            version,
            from,
            to,
            sequence,
            value,
            method_num,
            params,
            gas_limit,
            gas_fee_cap,
            gas_premium,
        }
    }
}

impl UnsignedMessage {
    pub fn builder() -> MessageBuilder {
        MessageBuilder::default()
    }

    /// Helper function to convert the message into signing bytes.
    /// This function returns the message `Cid` bytes.
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        // Safe to unwrap here, unsigned message cannot fail to serialize.
        self.cid().unwrap().to_bytes()
    }

    /// Semantic validation and validates the message has enough gas.
    #[cfg(feature = "proofs")]
    pub fn valid_for_block_inclusion(
        &self,
        min_gas: Gas,
        version: fil_types::NetworkVersion,
    ) -> Result<(), anyhow::Error> {
        use fil_types::{NetworkVersion, BLOCK_GAS_LIMIT, TOTAL_FILECOIN, ZERO_ADDRESS};
        use num_traits::Signed;
        if self.version != 0 {
            anyhow::bail!("Message version: {} not supported", self.version);
        }
        if self.to == *ZERO_ADDRESS && version >= NetworkVersion::V7 {
            anyhow::bail!("invalid 'to' address");
        }
        if self.value.is_negative() {
            anyhow::bail!("message value cannot be negative");
        }
        if self.value > *TOTAL_FILECOIN {
            anyhow::bail!("message value cannot be greater than total FIL supply");
        }
        if self.gas_fee_cap.is_negative() {
            anyhow::bail!("gas_fee_cap cannot be negative");
        }
        if self.gas_premium.is_negative() {
            anyhow::bail!("gas_premium cannot be negative");
        }
        if self.gas_premium > self.gas_fee_cap {
            anyhow::bail!("gas_fee_cap less than gas_premium");
        }
        if self.gas_limit > BLOCK_GAS_LIMIT {
            anyhow::bail!("gas_limit cannot be greater than block gas limit");
        }

        if Gas::new(self.gas_limit) < min_gas {
            anyhow::bail!(
                "gas_limit {} cannot be less than cost {} of storing a message on chain",
                self.gas_limit,
                min_gas
            );
        }

        Ok(())
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
            from,
            to,
            sequence,
            value,
            method_num,
            params,
            gas_limit,
            gas_fee_cap,
            gas_premium,
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
    use forest_address::json::AddressJson;
    use forest_bigint::bigint_ser;
    use forest_cid::Cid;
    use serde::{de, ser};

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
        to: AddressJson,
        from: AddressJson,
        #[serde(rename = "Nonce")]
        sequence: u64,
        #[serde(with = "bigint_ser::json")]
        value: TokenAmount,
        gas_limit: i64,
        #[serde(with = "bigint_ser::json")]
        gas_fee_cap: TokenAmount,
        #[serde(with = "bigint_ser::json")]
        gas_premium: TokenAmount,
        #[serde(rename = "Method")]
        method_num: u64,
        params: Option<String>,
        #[serde(default, rename = "CID", with = "forest_cid::json::opt")]
        cid: Option<Cid>,
    }

    pub fn serialize<S>(m: &UnsignedMessage, serializer: S) -> Result<S::Ok, S::Error>
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

    pub fn deserialize<'de, D>(deserializer: D) -> Result<UnsignedMessage, D::Error>
    where
        D: Deserializer<'de>,
    {
        let m: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(UnsignedMessage {
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
