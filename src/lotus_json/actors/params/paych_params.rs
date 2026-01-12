// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use fvm_ipld_encoding::RawBytes;
use pastey::paste;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ConstructorParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub from: Address,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub to: Address,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ModVerifyParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub actor: Address,
    pub method: u64,
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub data: RawBytes,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct MergeLotusJson {
    pub lane: u64,
    pub nonce: u64,
}

// Version-specific structs for different FVM versions to avoid conversion issues
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SignedVoucherV2LotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub channel_addr: Address,
    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub time_lock_min: ChainEpoch,
    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub time_lock_max: ChainEpoch,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json", rename = "SecretHash")]
    pub secret_pre_image: Vec<u8>,
    pub extra: Option<ModVerifyParamsLotusJson>,
    pub lane: u64,
    pub nonce: u64,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount: TokenAmount,
    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub min_settle_height: ChainEpoch,
    // Lotus returns null (not []) when there are no merges; model as Option, so None means empty.
    pub merges: Option<Vec<MergeLotusJson>>,
    #[schemars(with = "LotusJson<Option<fvm_shared2::crypto::signature::Signature>>")]
    #[serde(with = "crate::lotus_json")]
    pub signature: Option<fvm_shared2::crypto::signature::Signature>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SignedVoucherV3LotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub channel_addr: Address,
    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub time_lock_min: ChainEpoch,
    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub time_lock_max: ChainEpoch,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json", rename = "SecretHash")]
    pub secret_pre_image: Vec<u8>,
    pub extra: Option<ModVerifyParamsLotusJson>,
    pub lane: u64,
    pub nonce: u64,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount: TokenAmount,
    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub min_settle_height: ChainEpoch,
    // Lotus returns null (not []) when there are no merges; model as Option, so None means empty.
    pub merges: Option<Vec<MergeLotusJson>>,
    #[schemars(with = "LotusJson<Option<fvm_shared3::crypto::signature::Signature>>")]
    #[serde(with = "crate::lotus_json")]
    pub signature: Option<fvm_shared3::crypto::signature::Signature>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SignedVoucherV4LotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub channel_addr: Address,
    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub time_lock_min: ChainEpoch,
    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub time_lock_max: ChainEpoch,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json", rename = "SecretHash")]
    pub secret_pre_image: Vec<u8>,
    pub extra: Option<ModVerifyParamsLotusJson>,
    pub lane: u64,
    pub nonce: u64,
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub amount: TokenAmount,
    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub min_settle_height: ChainEpoch,
    // Lotus returns null (not []) when there are no merges; model as Option, so None means empty.
    pub merges: Option<Vec<MergeLotusJson>>,
    #[schemars(with = "LotusJson<Option<fvm_shared4::crypto::signature::Signature>>")]
    #[serde(with = "crate::lotus_json")]
    pub signature: Option<fvm_shared4::crypto::signature::Signature>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateChannelStateParamsV2LotusJson {
    pub sv: SignedVoucherV2LotusJson,
    #[serde(with = "crate::lotus_json")]
    pub secret: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateChannelStateParamsV3LotusJson {
    pub sv: SignedVoucherV3LotusJson,
    #[serde(with = "crate::lotus_json")]
    pub secret: Vec<u8>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateChannelStateParamsV4LotusJson {
    pub sv: SignedVoucherV4LotusJson,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    pub secret: Vec<u8>,
}

// Implementation for ConstructorParams
macro_rules! impl_paych_constructor_params {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_paych_constructor_params_ $version>] {
                    use super::*;
                    type T = fil_actor_paych_state::[<v $version>]::ConstructorParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = ConstructorParamsLotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({
                                        "From": "f01234",
                                        "To": "f01235"
                                    }),
                                    Self {
                                        from: Address::new_id(1234).into(),
                                        to: Address::new_id(1235).into(),
                                    },
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            ConstructorParamsLotusJson {
                                from: self.from.into(),
                                to: self.to.into(),
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                from: lotus_json.from.into(),
                                to: lotus_json.to.into(),
                            }
                        }
                    }
                }
            }
        )+
    };
}

// Implementation for UpdateChannelStateParams with version-specific types
macro_rules! impl_paych_update_channel_state_params_v2 {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_paych_update_channel_state_params_v2_ $version>] {
                    use super::*;
                    type T = fil_actor_paych_state::[<v $version>]::UpdateChannelStateParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = UpdateChannelStateParamsV2LotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({
                                        "Sv": {
                                            "ChannelAddr": "f01234",
                                            "TimeLockMin": 0,
                                            "TimeLockMax": 0,
                                            "SecretHash": null,
                                            "Extra": null,
                                            "Lane": 0,
                                            "Nonce": 1,
                                            "Amount": "1000",
                                            "MinSettleHeight": 0,
                                            "Merges": null,
                                            "Signature": null
                                        },
                                        "Secret": null
                                    }),
                                    Self {
                                        sv: fil_actor_paych_state::[<v $version>]::SignedVoucher {
                                            channel_addr: Address::new_id(1234).into(),
                                            time_lock_min: 0,
                                            time_lock_max: 0,
                                            secret_pre_image: vec![],
                                            extra: None,
                                            lane: 0,
                                            nonce: 1,
                                            amount: TokenAmount::from_atto(1000).into(),
                                            min_settle_height: 0,
                                            merges: vec![],
                                            signature: None,
                                        },
                                        secret: vec![],
                                    },
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            UpdateChannelStateParamsV2LotusJson {
                                sv: SignedVoucherV2LotusJson {
                                    channel_addr: self.sv.channel_addr.into(),
                                    time_lock_min: self.sv.time_lock_min,
                                    time_lock_max: self.sv.time_lock_max,
                                    secret_pre_image: self.sv.secret_pre_image,
                                    extra: self.sv.extra.map(|e| ModVerifyParamsLotusJson {
                                        actor: e.actor.into(),
                                        method: e.method,
                                        data: e.data,
                                    }),
                                    lane: self.sv.lane,
                                    nonce: self.sv.nonce,
                                    amount: self.sv.amount.into(),
                                    min_settle_height: self.sv.min_settle_height,
                                    merges: if self.sv.merges.is_empty() {
                                        None
                                    } else {
                                        Some(self.sv.merges.into_iter().map(|m| MergeLotusJson {
                                            lane: m.lane,
                                            nonce: m.nonce,
                                        }).collect())
                                    },
                                    signature: self.sv.signature,
                                },
                                secret: self.secret,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                sv: fil_actor_paych_state::[<v $version>]::SignedVoucher {
                                    channel_addr: lotus_json.sv.channel_addr.into(),
                                    time_lock_min: lotus_json.sv.time_lock_min,
                                    time_lock_max: lotus_json.sv.time_lock_max,
                                    secret_pre_image: lotus_json.sv.secret_pre_image,
                                    extra: lotus_json.sv.extra.map(|e| fil_actor_paych_state::[<v $version>]::ModVerifyParams {
                                        actor: e.actor.into(),
                                        method: e.method,
                                        data: e.data,
                                    }),
                                    lane: lotus_json.sv.lane,
                                    nonce: lotus_json.sv.nonce,
                                    amount: lotus_json.sv.amount.into(),
                                    min_settle_height: lotus_json.sv.min_settle_height,
                                    merges: if lotus_json.sv.merges.is_none() {
                                        vec![]
                                    } else {
                                        lotus_json.sv.merges.unwrap().into_iter().map(|m| fil_actor_paych_state::[<v $version>]::Merge {
                                            lane: m.lane,
                                            nonce: m.nonce,
                                        }).collect()
                                    },
                                    signature: lotus_json.sv.signature,
                                },
                                secret: lotus_json.secret,
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_paych_update_channel_state_params_v3 {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_paych_update_channel_state_params_v3_ $version>] {
                    use super::*;
                    type T = fil_actor_paych_state::[<v $version>]::UpdateChannelStateParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = UpdateChannelStateParamsV3LotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({
                                        "Sv": {
                                            "ChannelAddr": "f01234",
                                            "TimeLockMin": 0,
                                            "TimeLockMax": 0,
                                            "SecretHash": null,
                                            "Extra": null,
                                            "Lane": 0,
                                            "Nonce": 1,
                                            "Amount": "1000",
                                            "MinSettleHeight": 0,
                                            "Merges": null,
                                            "Signature": null
                                        },
                                        "Secret": null
                                    }),
                                    Self {
                                        sv: fil_actor_paych_state::[<v $version>]::SignedVoucher {
                                            channel_addr: Address::new_id(1234).into(),
                                            time_lock_min: 0,
                                            time_lock_max: 0,
                                            secret_pre_image: vec![],
                                            extra: None,
                                            lane: 0,
                                            nonce: 1,
                                            amount: TokenAmount::from_atto(1000).into(),
                                            min_settle_height: 0,
                                            merges: vec![],
                                            signature: None,
                                        },
                                        secret: vec![],
                                    },
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            UpdateChannelStateParamsV3LotusJson {
                                sv: SignedVoucherV3LotusJson {
                                    channel_addr: self.sv.channel_addr.into(),
                                    time_lock_min: self.sv.time_lock_min,
                                    time_lock_max: self.sv.time_lock_max,
                                    secret_pre_image: self.sv.secret_pre_image,
                                    extra: self.sv.extra.map(|e| ModVerifyParamsLotusJson {
                                        actor: e.actor.into(),
                                        method: e.method,
                                        data: e.data,
                                    }),
                                    lane: self.sv.lane,
                                    nonce: self.sv.nonce,
                                    amount: self.sv.amount.into(),
                                    min_settle_height: self.sv.min_settle_height,
                                    merges: if self.sv.merges.is_empty() {
                                        None
                                    } else {
                                        Some(self.sv.merges.into_iter().map(|m| MergeLotusJson {
                                            lane: m.lane,
                                            nonce: m.nonce,
                                        }).collect())
                                    },
                                    signature: self.sv.signature,
                                },
                                secret: self.secret,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                sv: fil_actor_paych_state::[<v $version>]::SignedVoucher {
                                    channel_addr: lotus_json.sv.channel_addr.into(),
                                    time_lock_min: lotus_json.sv.time_lock_min,
                                    time_lock_max: lotus_json.sv.time_lock_max,
                                    secret_pre_image: lotus_json.sv.secret_pre_image,
                                    extra: lotus_json.sv.extra.map(|e| fil_actor_paych_state::[<v $version>]::ModVerifyParams {
                                        actor: e.actor.into(),
                                        method: e.method,
                                        data: e.data,
                                    }),
                                    lane: lotus_json.sv.lane,
                                    nonce: lotus_json.sv.nonce,
                                    amount: lotus_json.sv.amount.into(),
                                    min_settle_height: lotus_json.sv.min_settle_height,
                                    merges: if lotus_json.sv.merges.is_none() {
                                        vec![]
                                    } else {
                                        lotus_json.sv.merges.unwrap().into_iter().map(|m| fil_actor_paych_state::[<v $version>]::Merge {
                                            lane: m.lane,
                                            nonce: m.nonce,
                                        }).collect()
                                    },
                                    signature: lotus_json.sv.signature,
                                },
                                secret: lotus_json.secret,
                            }
                        }
                    }
                }
            }
        )+
    };
}

macro_rules! impl_paych_update_channel_state_params_v4 {
    ($($version:literal),+) => {
        $(
            paste! {
                mod [<impl_paych_update_channel_state_params_v4_ $version>] {
                    use super::*;
                    type T = fil_actor_paych_state::[<v $version>]::UpdateChannelStateParams;
                    #[test]
                    fn snapshots() {
                        crate::lotus_json::assert_all_snapshots::<T>();
                    }

                    impl HasLotusJson for T {
                        type LotusJson = UpdateChannelStateParamsV4LotusJson;

                        #[cfg(test)]
                        fn snapshots() -> Vec<(serde_json::Value, Self)> {
                            vec![
                                (
                                    json!({
                                        "Sv": {
                                            "ChannelAddr": "f01234",
                                            "TimeLockMin": 0,
                                            "TimeLockMax": 0,
                                            "SecretHash": null,
                                            "Extra": null,
                                            "Lane": 0,
                                            "Nonce": 1,
                                            "Amount": "1000",
                                            "MinSettleHeight": 0,
                                            "Merges": null,
                                            "Signature": null
                                        },
                                        "Secret": null
                                    }),
                                    Self {
                                        sv: fil_actor_paych_state::[<v $version>]::SignedVoucher {
                                            channel_addr: Address::new_id(1234).into(),
                                            time_lock_min: 0,
                                            time_lock_max: 0,
                                            secret_pre_image: vec![],
                                            extra: None,
                                            lane: 0,
                                            nonce: 1,
                                            amount: TokenAmount::from_atto(1000).into(),
                                            min_settle_height: 0,
                                            merges: vec![],
                                            signature: None,
                                        },
                                        secret: vec![],
                                    },
                                ),
                            ]
                        }

                        fn into_lotus_json(self) -> Self::LotusJson {
                            UpdateChannelStateParamsV4LotusJson {
                                sv: SignedVoucherV4LotusJson {
                                    channel_addr: self.sv.channel_addr.into(),
                                    time_lock_min: self.sv.time_lock_min,
                                    time_lock_max: self.sv.time_lock_max,
                                    secret_pre_image: self.sv.secret_pre_image,
                                    extra: self.sv.extra.map(|e| ModVerifyParamsLotusJson {
                                        actor: e.actor.into(),
                                        method: e.method,
                                        data: e.data,
                                    }),
                                    lane: self.sv.lane,
                                    nonce: self.sv.nonce,
                                    amount: self.sv.amount.into(),
                                    min_settle_height: self.sv.min_settle_height,
                                    merges: if self.sv.merges.is_empty() {
                                        None
                                    } else {
                                        Some(self.sv.merges.into_iter().map(|m| MergeLotusJson {
                                            lane: m.lane,
                                            nonce: m.nonce,
                                        }).collect())
                                    },
                                    signature: self.sv.signature,
                                },
                                secret: self.secret,
                            }
                        }

                        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                            Self {
                                sv: fil_actor_paych_state::[<v $version>]::SignedVoucher {
                                    channel_addr: lotus_json.sv.channel_addr.into(),
                                    time_lock_min: lotus_json.sv.time_lock_min,
                                    time_lock_max: lotus_json.sv.time_lock_max,
                                    secret_pre_image: lotus_json.sv.secret_pre_image,
                                    extra: lotus_json.sv.extra.map(|e| fil_actor_paych_state::[<v $version>]::ModVerifyParams {
                                        actor:  e.actor.into(),
                                        method: e.method,
                                        data: e.data,
                                    }),
                                    lane: lotus_json.sv.lane,
                                    nonce: lotus_json.sv.nonce,
                                    amount: lotus_json.sv.amount.into(),
                                    min_settle_height: lotus_json.sv.min_settle_height,
                                    merges: if lotus_json.sv.merges.is_none() {
                                        vec![]
                                    } else {
                                        lotus_json.sv.merges.unwrap().into_iter().map(|m| fil_actor_paych_state::[<v $version>]::Merge {
                                            lane: m.lane,
                                            nonce: m.nonce,
                                        }).collect()
                                    },
                                    signature: lotus_json.sv.signature,
                                },
                                secret: lotus_json.secret,
                            }
                        }
                    }
                }
            }
        )+
    };
}

// Apply implementations with correct fvm_shared versions
impl_paych_constructor_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_paych_update_channel_state_params_v2!(8, 9);
impl_paych_update_channel_state_params_v3!(10, 11);
impl_paych_update_channel_state_params_v4!(12, 13, 14, 15, 16, 17);
