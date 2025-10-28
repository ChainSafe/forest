// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::{clock::ChainEpoch, econ::TokenAmount};
use ::cid::Cid;
use pastey::paste;

// Single LotusJson struct for VestingFund (used by all versions)
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "VestingFund")]
pub struct VestingFundLotusJson {
    pub epoch: ChainEpoch,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount: TokenAmount,
}

// Single LotusJson struct for VestingFunds (used by all versions)
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "VestingFunds")]
pub struct VestingFundsLotusJson {
    pub head: VestingFundLotusJson,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub tail: Cid,
}

// Macro to implement HasLotusJson for VestingFund and VestingFunds
macro_rules! impl_vesting_funds_lotus_json {
    ($($version_num:literal),+) => {
        $(
        paste! {
            // Implement HasLotusJson for VestingFund
            impl HasLotusJson for fil_actor_miner_state::[<v $version_num>]::VestingFund {
                type LotusJson = VestingFundLotusJson;

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
                    VestingFundLotusJson {
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

            // Implement HasLotusJson for VestingFunds
            impl HasLotusJson for fil_actor_miner_state::[<v $version_num>]::VestingFunds {
                type LotusJson = Option<VestingFundsLotusJson>;

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
                            Self(Some(fil_actor_miner_state::[<v $version_num>]::VestingFundsInner {
                                head: fil_actor_miner_state::[<v $version_num>]::VestingFund {
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
                    self.0.map(|v| VestingFundsLotusJson {
                        head: VestingFundLotusJson {
                            epoch: v.head.epoch,
                            amount: v.head.amount.into(),
                        },
                        tail: v.tail,
                    })
                }

                fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                    match lotus_json {
                        None => Self(None),
                        Some(json) => Self(Some(fil_actor_miner_state::[<v $version_num>]::VestingFundsInner {
                            head: fil_actor_miner_state::[<v $version_num>]::VestingFund {
                                epoch: json.head.epoch,
                                amount: json.head.amount.into(),
                            },
                            tail: json.tail,
                        })),
                    }
                }
            }
        }
        )+
    };
}

impl_vesting_funds_lotus_json!(16, 17);
