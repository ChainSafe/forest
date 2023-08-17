// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::interpreter::MessageGasCost;

pub mod json {
    use cid::Cid;
    use num_bigint::BigInt;
    use std::str::FromStr;

    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    use crate::shim::econ::TokenAmount;

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
        fn from(mgc: MessageGasCost) -> Self {
            MessageGasCostJson(mgc)
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        #[serde(default)]
        pub message: Option<Cid>,
        pub gas_used: String,
        #[serde(with = "crate::json::token_amount::json")]
        pub base_fee_burn: TokenAmount,
        #[serde(with = "crate::json::token_amount::json")]
        pub over_estimation_burn: TokenAmount,
        #[serde(with = "crate::json::token_amount::json")]
        pub miner_penalty: TokenAmount,
        #[serde(with = "crate::json::token_amount::json")]
        pub miner_tip: TokenAmount,
        #[serde(with = "crate::json::token_amount::json")]
        pub refund: TokenAmount,
        #[serde(with = "crate::json::token_amount::json")]
        pub total_cost: TokenAmount,
    }

    pub fn serialize<S>(mgc: &MessageGasCost, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            message: mgc.message,
            gas_used: mgc.gas_used.to_str_radix(10),
            base_fee_burn: mgc.base_fee_burn.clone(),
            over_estimation_burn: mgc.over_estimation_burn.clone(),
            miner_penalty: mgc.miner_penalty.clone(),
            miner_tip: mgc.miner_tip.clone(),
            refund: mgc.refund.clone(),
            total_cost: mgc.total_cost.clone(),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<MessageGasCost, D::Error>
    where
        D: Deserializer<'de>,
    {
        let h: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(MessageGasCost {
            message: h.message,
            gas_used: BigInt::from_str(&h.gas_used).map_err(de::Error::custom)?,
            base_fee_burn: h.base_fee_burn,
            over_estimation_burn: h.over_estimation_burn,
            miner_penalty: h.miner_penalty,
            miner_tip: h.miner_tip,
            refund: h.refund,
            total_cost: h.total_cost,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::shim::econ::TokenAmount;
    use num_bigint::BigInt;

    use quickcheck_macros::quickcheck;

    use super::*;

    impl quickcheck::Arbitrary for MessageGasCost {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            Self {
                message: Option::arbitrary(g),
                gas_used: BigInt::arbitrary(g),
                base_fee_burn: TokenAmount::arbitrary(g),
                over_estimation_burn: TokenAmount::arbitrary(g),
                miner_penalty: TokenAmount::arbitrary(g),
                miner_tip: TokenAmount::arbitrary(g),
                refund: TokenAmount::arbitrary(g),
                total_cost: TokenAmount::arbitrary(g),
            }
        }
    }

    #[quickcheck]
    fn message_gas_cost_roundtrip(mgc: MessageGasCost) {
        let serialized = crate::to_string_with!(&mgc, json::serialize);
        let parsed: MessageGasCost = crate::from_str_with!(&serialized, json::deserialize);
        assert_eq!(mgc, parsed);
    }
}
