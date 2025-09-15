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

impl HasLotusJson for State {
    type LotusJson = PaymentChannelStateLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        use crate::shim::address::Address;
        vec![(
            json!({
                "From": "f00",
                "To": "f01",
                "ToSend": "0",
                "SettlingAt": 0,
                "MinSettleHeight": 0,
                "LaneStates": {"/":"baeaaaaa"}
            }),
            State::default_latest_version(
                Address::new_id(0).into(),
                Address::new_id(1).into(),
                TokenAmount::default().into(),
                0,
                0,
                Cid::default(),
            ),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        macro_rules! convert_payment_channel_state {
            ($($version:ident),+) => {
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
                    )+
                }
            };
        }

        convert_payment_channel_state!(V9, V10, V11, V12, V13, V14, V15, V16, V17)
    }

    // Always return the latest version when deserializing
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        State::default_latest_version(
            lotus_json.from.into(),
            lotus_json.to.into(),
            lotus_json.to_send.into(),
            lotus_json.settling_at,
            lotus_json.min_settle_height,
            lotus_json.lane_states,
        )
    }
}
