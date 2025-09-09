// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::paymentchannel::State;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use ::cid::Cid;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "PaymentChannelState")]
pub struct PaymentChannelStateLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub from: Address,

    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub to: Address,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub to_send: TokenAmount,

    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub settling_at: ChainEpoch,

    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub min_settle_height: ChainEpoch,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub lane_states: Cid,
}

macro_rules! impl_paych_state_lotus_json {
    ($($version:ident),*) => {
        impl HasLotusJson for State {
            type LotusJson = PaymentChannelStateLotusJson;

            #[cfg(test)]
            fn snapshots() -> Vec<(serde_json::Value, Self)> {
                todo!()
            }

            fn into_lotus_json(self) -> Self::LotusJson {
                match self {
                    $(
                        State::$version(state) => PaymentChannelStateLotusJson {
                            from: state.from.into(),
                            to: state.to.into(),
                            to_send: state.to_send.into(),
                            settling_at: state.settling_at,
                            min_settle_height: state.min_settle_height,
                            lane_states: state.lane_states,
                        },
                    )*
                }
            }

            // Default to V16
            fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                State::V16(fil_actor_paych_state::v16::State {
                    from: lotus_json.from.into(),
                    to: lotus_json.to.into(),
                    to_send: lotus_json.to_send.into(),
                    settling_at: lotus_json.settling_at,
                    min_settle_height: lotus_json.min_settle_height,
                    lane_states: lotus_json.lane_states,
                })
            }
        }
    };
}

impl_paych_state_lotus_json!(V9, V10, V11, V12, V13, V14, V15, V16, V17);
