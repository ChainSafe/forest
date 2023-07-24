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
        fn from(ir: MessageGasCost) -> Self {
            MessageGasCostJson(ir)
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        #[serde(default, with = "crate::json::cid")]
        pub message: Cid,
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

    pub fn serialize<S>(gc: &MessageGasCost, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            message: gc.message,
            gas_used: gc.gas_used.to_str_radix(10),
            base_fee_burn: gc.base_fee_burn.clone(),
            over_estimation_burn: gc.over_estimation_burn.clone(),
            miner_penalty: gc.miner_penalty.clone(),
            miner_tip: gc.miner_tip.clone(),
            refund: gc.refund.clone(),
            total_cost: gc.total_cost.clone(),
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
            gas_used: BigInt::from_str(&gc.gas_used).map_err(de::Error::custom)?,
            base_fee_burn: gc.base_fee_burn,
            over_estimation_burn: gc.over_estimation_burn,
            miner_penalty: gc.miner_penalty,
            miner_tip: gc.miner_tip,
            refund: gc.refund,
            total_cost: gc.total_cost,
        })
    }
}

#[cfg(test)]
pub mod tests {
    // todo!
}
