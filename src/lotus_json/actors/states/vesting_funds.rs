// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use crate::shim::{clock::ChainEpoch, econ::TokenAmount};
use ::cid::Cid;
use fil_actor_miner_state::v16::{
    VestingFund as VestingFundV16, VestingFundsInner as VestingFundsInnerV16,
};
use fil_actor_miner_state::v17::{
    VestingFund as VestingFundV17, VestingFundsInner as VestingFundsInnerV17,
};

#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "VestingFund")]
pub struct VestingFundV16LotusJson {
    pub epoch: ChainEpoch,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount: TokenAmount,
}

impl HasLotusJson for VestingFundV16 {
    type LotusJson = VestingFundV16LotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        use fvm_shared4::bigint::BigInt;

        vec![
            (
                json!({
                    "Epoch": 1000,
                    "Amount": "0"
                }),
                Self {
                    epoch: 1000,
                    amount: Default::default(),
                },
            ),
            (
                json!({
                    "Epoch": 2000,
                    "Amount": "1000000000000000000"
                }),
                Self {
                    epoch: 2000,
                    amount: TokenAmount::from_atto(BigInt::from(10u64.pow(18))).into(),
                },
            ),
        ]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        VestingFundV16LotusJson {
            epoch: self.epoch,
            amount: self.amount.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            epoch: lotus_json.epoch,
            amount: lotus_json.amount.into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "VestingFund")]
pub struct VestingFundV17LotusJson {
    pub epoch: ChainEpoch,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount: TokenAmount,
}

impl HasLotusJson for VestingFundV17 {
    type LotusJson = VestingFundV17LotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        use fvm_shared4::bigint::BigInt;

        vec![
            (
                json!({
                    "Epoch": 1000,
                    "Amount": "0"
                }),
                Self {
                    epoch: 1000,
                    amount: Default::default(),
                },
            ),
            (
                json!({
                    "Epoch": 2000,
                    "Amount": "1000000000000000000"
                }),
                Self {
                    epoch: 2000,
                    amount: TokenAmount::from_atto(BigInt::from(10u64.pow(18))).into(),
                },
            ),
        ]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        VestingFundV17LotusJson {
            epoch: self.epoch,
            amount: self.amount.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            epoch: lotus_json.epoch,
            amount: lotus_json.amount.into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "VestingFunds")]
pub struct VestingFundsV16LotusJson {
    #[schemars(with = "LotusJson<VestingFundV16>")]
    #[serde(with = "crate::lotus_json")]
    pub head: VestingFundV16,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub tail: Cid,
}

impl HasLotusJson for fil_actor_miner_state::v16::VestingFunds {
    type LotusJson = Option<VestingFundsV16LotusJson>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![
            (json!(null), Self(None)),
            (
                json!({
                    "Head": {
                        "Epoch": 1000,
                        "Amount": "1000000000000000000"
                    },
                    "Tail": "bafy2bzaceaa43t4wykyk57ibfghjkvcbartledtcflp25htn56svwkrtp6ddy"
                }),
                Self(Some(VestingFundsInnerV16 {
                    head: VestingFundV16 {
                        epoch: 1000,
                        amount: TokenAmount::from_atto(num_bigint::BigInt::from(10u64.pow(18)))
                            .into(),
                    },
                    tail: Cid::try_from(
                        "bafy2bzaceaa43t4wykyk57ibfghjkvcbartledtcflp25htn56svwkrtp6ddy",
                    )
                    .unwrap(),
                })),
            ),
        ]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.0.map(|v| VestingFundsV16LotusJson {
            head: VestingFundV16 {
                epoch: v.head.epoch,
                amount: v.head.amount,
            },
            tail: v.tail,
        })
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        match lotus_json {
            None => Self(None),
            Some(json) => Self(Some(VestingFundsInnerV16 {
                head: VestingFundV16 {
                    epoch: json.head.epoch,
                    amount: json.head.amount,
                },
                tail: json.tail,
            })),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "VestingFunds")]
pub struct VestingFundsV17LotusJson {
    #[schemars(with = "LotusJson<VestingFundV17>")]
    #[serde(with = "crate::lotus_json")]
    pub head: VestingFundV17,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub tail: Cid,
}

impl HasLotusJson for fil_actor_miner_state::v17::VestingFunds {
    type LotusJson = Option<VestingFundsV17LotusJson>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![
            (json!(null), Self(None)),
            (
                json!({
                    "Head": {
                        "Epoch": 1000,
                        "Amount": "1000000000000000000"
                    },
                    "Tail": "bafy2bzaceaa43t4wykyk57ibfghjkvcbartledtcflp25htn56svwkrtp6ddy"
                }),
                Self(Some(VestingFundsInnerV17 {
                    head: VestingFundV17 {
                        epoch: 1000,
                        amount: TokenAmount::from_atto(num_bigint::BigInt::from(10u64.pow(18)))
                            .into(),
                    },
                    tail: Cid::try_from(
                        "bafy2bzaceaa43t4wykyk57ibfghjkvcbartledtcflp25htn56svwkrtp6ddy",
                    )
                    .unwrap(),
                })),
            ),
        ]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        self.0.map(|v| VestingFundsV17LotusJson {
            head: VestingFundV17 {
                epoch: v.head.epoch,
                amount: v.head.amount,
            },
            tail: v.tail,
        })
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        match lotus_json {
            None => Self(None),
            Some(json) => Self(Some(VestingFundsInnerV17 {
                head: VestingFundV17 {
                    epoch: json.head.epoch,
                    amount: json.head.amount,
                },
                tail: json.tail,
            })),
        }
    }
}
