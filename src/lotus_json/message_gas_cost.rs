// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use ::cid::Cid;
use num::BigInt;

use super::*;

use crate::{interpreter::MessageGasCost, shim::econ::TokenAmount};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageGasCostLotusJson {
    #[serde(skip_serializing_if = "LotusJson::is_none", default)]
    message: LotusJson<Option<Cid>>,
    gas_used: LotusJson<BigInt>,
    base_fee_burn: LotusJson<TokenAmount>,
    over_estimation_burn: LotusJson<TokenAmount>,
    miner_penalty: LotusJson<TokenAmount>,
    miner_tip: LotusJson<TokenAmount>,
    refund: LotusJson<TokenAmount>,
    total_cost: LotusJson<TokenAmount>,
}

impl HasLotusJson for MessageGasCost {
    type LotusJson = MessageGasCostLotusJson;

    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "BaseFeeBurn": "0",
                "GasUsed": "0",
                "MinerPenalty": "0",
                "MinerTip": "0",
                "OverEstimationBurn": "0",
                "Refund": "0",
                "TotalCost": "0"
            }),
            Self {
                message: None,
                gas_used: BigInt::from(0),
                base_fee_burn: TokenAmount::from_atto(0),
                over_estimation_burn: TokenAmount::from_atto(0),
                miner_penalty: TokenAmount::from_atto(0),
                miner_tip: TokenAmount::from_atto(0),
                refund: TokenAmount::from_atto(0),
                total_cost: TokenAmount::from_atto(0),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let Self {
            message,
            gas_used,
            base_fee_burn,
            over_estimation_burn,
            miner_penalty,
            miner_tip,
            refund,
            total_cost,
        } = self;
        Self::LotusJson {
            message: message.into(),
            gas_used: gas_used.into(),
            base_fee_burn: base_fee_burn.into(),
            over_estimation_burn: over_estimation_burn.into(),
            miner_penalty: miner_penalty.into(),
            miner_tip: miner_tip.into(),
            refund: refund.into(),
            total_cost: total_cost.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            message,
            gas_used,
            base_fee_burn,
            over_estimation_burn,
            miner_penalty,
            miner_tip,
            refund,
            total_cost,
        } = lotus_json;
        Self {
            message: message.into_inner(),
            gas_used: gas_used.into_inner(),
            base_fee_burn: base_fee_burn.into_inner(),
            over_estimation_burn: over_estimation_burn.into_inner(),
            miner_penalty: miner_penalty.into_inner(),
            miner_tip: miner_tip.into_inner(),
            refund: refund.into_inner(),
            total_cost: total_cost.into_inner(),
        }
    }
}
