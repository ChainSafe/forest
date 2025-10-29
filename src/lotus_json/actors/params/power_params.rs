// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use crate::shim::sector::RegisteredPoStProof;
use fvm_ipld_encoding::{BytesDe, RawBytes};
use fvm_shared4::ActorID;
use num::BigInt;
use pastey::paste;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct CreateMinerParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub owner: Address,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub worker: Address,
    #[schemars(with = "LotusJson<RegisteredPoStProof>")]
    #[serde(with = "crate::lotus_json")]
    pub window_po_st_proof_type: RegisteredPoStProof,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    pub peer: Vec<u8>,
    #[schemars(with = "LotusJson<Vec<Vec<u8>>>")]
    #[serde(with = "crate::lotus_json")]
    pub multiaddrs: Vec<Vec<u8>>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateClaimedPowerParamsLotusJson {
    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub raw_byte_delta: BigInt,
    #[schemars(with = "LotusJson<BigInt>")]
    #[serde(with = "crate::lotus_json")]
    pub quality_adjusted_delta: BigInt,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct EnrollCronEventParamsLotusJson {
    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub event_epoch: ChainEpoch,
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub payload: RawBytes,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct UpdatePledgeTotalParamsLotusJson(
    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    TokenAmount,
);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct MinerRawPowerParamsLotusJson(ActorID);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct MinerPowerParamsLotusJson {
    #[schemars(with = "LotusJson<ActorID>")]
    #[serde(with = "crate::lotus_json")]
    pub miner: ActorID,
}

// Implementations for CreateMinerParams
macro_rules! impl_lotus_json_for_power_create_miner_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_power_state::[<v $version>]::CreateMinerParams {
                    type LotusJson = CreateMinerParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!({
                                    "Owner": "f01234",
                                    "Worker": "f01235",
                                    "WindowPostProofType": 1,
                                    "Peer": "AQ==",
                                    "Multiaddrs": ["Ag==", "Aw=="],
                                }),
                                Self {
                                    owner: Address::new_id(1234).into(),
                                    worker: Address::new_id(1235).into(),
                                    window_post_proof_type: RegisteredPoStProof::from(fvm_shared4::sector::RegisteredPoStProof::StackedDRGWindow2KiBV1P1).into(),
                                    peer: vec![1],
                                    multiaddrs: vec![],
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        CreateMinerParamsLotusJson {
                            owner: self.owner.into(),
                            worker: self.worker.into(),
                            window_po_st_proof_type: self.window_post_proof_type.into(),
                            peer: self.peer,
                            multiaddrs: self.multiaddrs.into_iter().map(|addr| addr.0).collect(),
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            owner: lotus_json.owner.into(),
                            worker: lotus_json.worker.into(),
                            window_post_proof_type: lotus_json.window_po_st_proof_type.into(),
                            peer: lotus_json.peer,
                            multiaddrs: lotus_json.multiaddrs.into_iter().map(BytesDe).collect(),
                        }
                    }
                }
            }
        )+
    };
}

// Implementations for UpdateClaimedPowerParams
macro_rules! impl_lotus_json_for_power_update_claimed_power_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_power_state::[<v $version>]::UpdateClaimedPowerParams {
                    type LotusJson = UpdateClaimedPowerParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!({
                                    "RawByteDelta": "1024",
                                    "QualityAdjustedDelta": "2048",
                                }),
                                Self {
                                    raw_byte_delta: crate::shim::sector::StoragePower::from(1024u64),
                                    quality_adjusted_delta: crate::shim::sector::StoragePower::from(2048u64),
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        UpdateClaimedPowerParamsLotusJson {
                            raw_byte_delta: self.raw_byte_delta,
                            quality_adjusted_delta: self.quality_adjusted_delta,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            raw_byte_delta: lotus_json.raw_byte_delta,
                            quality_adjusted_delta: lotus_json.quality_adjusted_delta,
                        }
                    }
                }
            }
        )+
    };
}

// Implementations for EnrollCronEventParams
macro_rules! impl_lotus_json_for_power_enroll_cron_event_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_power_state::[<v $version>]::EnrollCronEventParams {
                    type LotusJson = EnrollCronEventParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!({
                                    "EventEpoch": 12345,
                                    "Payload": "ESIzRFU=",
                                }),
                                Self {
                                    event_epoch: 12345,
                                    payload: RawBytes::new(hex::decode("1122334455").unwrap()),
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        EnrollCronEventParamsLotusJson {
                            event_epoch: self.event_epoch,
                            payload: self.payload,
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            event_epoch: lotus_json.event_epoch,
                            payload: lotus_json.payload,
                        }
                    }
                }
            }
        )+
    };
}

// Implementations for UpdatePledgeTotalParams
macro_rules! impl_lotus_json_for_power_update_pledge_total_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_power_state::[<v $version>]::UpdatePledgeTotalParams {
                    type LotusJson = UpdatePledgeTotalParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!({
                                    "PledgeDelta": "1000000000000000000",
                                }),
                                Self {
                                    pledge_delta: TokenAmount::from_atto(1000000000000000000u64).into(),
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        UpdatePledgeTotalParamsLotusJson(self.pledge_delta.into())
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            pledge_delta: lotus_json.0.into(),
                        }
                    }
                }
            }
        )+
    };
}

// Implementations for MinerRawPowerParams
macro_rules! impl_lotus_json_for_power_miner_raw_power_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_power_state::[<v $version>]::MinerRawPowerParams {
                    type LotusJson = MinerRawPowerParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!({
                                    "Miner": 1001,
                                }),
                                Self {
                                    miner: 1001,
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        MinerRawPowerParamsLotusJson(self.miner)
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            miner: lotus_json.0,
                        }
                    }
                }
            }
        )+
    };
}

// Implementations for MinerPowerParams (only present in the power actor v16 and v17)
impl HasLotusJson for fil_actor_power_state::v16::MinerPowerParams {
    type LotusJson = MinerPowerParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Miner": 1002,
            }),
            Self { miner: 1002 },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        MinerPowerParamsLotusJson { miner: self.miner }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            miner: lotus_json.miner,
        }
    }
}

impl HasLotusJson for fil_actor_power_state::v17::MinerPowerParams {
    type LotusJson = MinerPowerParamsLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({
                "Miner": 1002,
            }),
            Self { miner: 1002 },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        MinerPowerParamsLotusJson { miner: self.miner }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Self {
            miner: lotus_json.miner,
        }
    }
}

impl_lotus_json_for_power_create_miner_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_power_update_claimed_power_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_power_enroll_cron_event_params!(8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_power_update_pledge_total_params!(10, 11, 12, 13, 14, 15, 16, 17);
impl_lotus_json_for_power_miner_raw_power_params!(10, 11, 12, 13, 14, 15, 16, 17);
