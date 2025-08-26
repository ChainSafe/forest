// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::piece::PaddedPieceSize;
use crate::shim::sector::SectorNumber;
use fvm_ipld_encoding::RawBytes;
use paste::paste;

use ::cid::Cid;

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct MinerChangeWorkerParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub new_worker: Address,

    #[schemars(with = "LotusJson<Vec<Address>>")]
    #[serde(with = "crate::lotus_json")]
    #[serde(rename = "NewControlAddrs")]
    pub new_control_addresses: Vec<Address>,
}

macro_rules!  impl_lotus_json_for_miner_change_worker_param {
    ($($version:literal),+) => {
        $(
        paste! {
                impl HasLotusJson for fil_actor_miner_state::[<v $version>]::ChangeWorkerAddressParams {
                    type LotusJson = MinerChangeWorkerParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!({
                                    "NewWorker": "f01234",
                                    "NewControlAddrs": ["f01236", "f01237"],
                                }),
                                Self {
                                    new_worker: Address::new_id(1234).into(),
                                    new_control_addresses: vec![Address::new_id(1236).into(), Address::new_id(1237).into()],
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        MinerChangeWorkerParamsLotusJson {
                            new_worker: self.new_worker.into(),
                            new_control_addresses: self.new_control_addresses
                                .into_iter()
                                .map(|a| a.into())
                                .collect(),
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            new_worker: lotus_json.new_worker.into(),
                            new_control_addresses: lotus_json.new_control_addresses
                                .into_iter()
                                .map(|a| a.into())
                                .collect(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_miner_change_worker_param!(8, 9, 10, 11, 12, 13, 14, 15, 16);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct PieceChangeLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub data: Cid,
    #[schemars(with = "LotusJson<PaddedPieceSize>")]
    #[serde(with = "crate::lotus_json")]
    pub size: PaddedPieceSize,
    #[schemars(with = "LotusJson<RawBytes>")]
    #[serde(with = "crate::lotus_json")]
    pub payload: RawBytes,
}

macro_rules! impl_lotus_json_piece_change {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_miner_state::[<v $version>]::PieceChange {
                    type LotusJson = PieceChangeLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            data: self.data.into(),
                            size: self.size.into(),
                            payload: self.payload.into(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            data: json.data.into(),
                            size: json.size.into(),
                            payload: json.payload.into(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_piece_change!(13, 14, 15, 16);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SectorChangesLotusJson {
    #[schemars(with = "LotusJson<Vec<Address>>")]
    #[serde(with = "crate::lotus_json")]
    pub sector: SectorNumber,
    #[schemars(with = "LotusJson<ChainEpoch>")]
    #[serde(with = "crate::lotus_json")]
    pub minimum_commitment_epoch: ChainEpoch,
    pub added: Vec<PieceChangeLotusJson>,
}

macro_rules! impl_lotus_json_sector_changes {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_miner_state::[<v $version>]::SectorChanges {
                    type LotusJson = SectorChangesLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            sector: self.sector.into(),
                            minimum_commitment_epoch: self.minimum_commitment_epoch.into(),
                            added: self.added.into_iter()
                                .map(|pc| pc.into_lotus_json())
                                .collect(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            sector: json.sector.into(),
                            minimum_commitment_epoch: json.minimum_commitment_epoch.into(),
                            added: json.added.into_iter()
                                .map(|pc| fil_actor_miner_state::[<v $version>]::PieceChange::from_lotus_json(pc),)
                                .collect(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_sector_changes!(13, 14, 15, 16);

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SectorContentChangedParamsLotusJson {
    pub sectors: Vec<SectorChangesLotusJson>,
}

macro_rules! impl_lotus_json_sector_content_changed_params {
    ($($version:literal),+) => {
        $(
            paste! {
                impl HasLotusJson for fil_actor_miner_state::[<v $version>]::SectorContentChangedParams {
                    type LotusJson = SectorContentChangedParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        Self::LotusJson {
                            sectors: self.sectors.into_iter()
                                .map(|sc| sc.into_lotus_json())
                                .collect(),
                        }
                    }

                    fn from_lotus_json(json: Self::LotusJson) -> Self {
                        Self {
                            sectors: json.sectors.into_iter()
                                .map(|sc| fil_actor_miner_state::[<v $version>]::SectorChanges::from_lotus_json(sc),)
                                .collect(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_sector_content_changed_params!(13, 14, 15, 16);
