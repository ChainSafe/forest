// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use crate::shim::address::Address;
use crate::shim::sector::RegisteredPoStProof;
use fvm_ipld_encoding::BytesDe;
use paste::paste;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct MinerConstructorParamsLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub owner_addr: Address,
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub worker_addr: Address,
    #[schemars(with = "LotusJson<Vec<Address>>")]
    #[serde(with = "crate::lotus_json")]
    pub control_addrs: Vec<Address>,
    #[schemars(with = "LotusJson<RegisteredPoStProof>")]
    #[serde(with = "crate::lotus_json")]
    pub window_post_proof_type: RegisteredPoStProof,
    #[schemars(with = "LotusJson<Vec<u8>>")]
    #[serde(with = "crate::lotus_json")]
    pub peer_id: Vec<u8>,
    #[schemars(with = "LotusJson<Vec<Vec<u8>>>")]
    #[serde(with = "crate::lotus_json")]
    pub multiaddrs: Vec<Vec<u8>>,
}

macro_rules! impl_lotus_json_for_miner_constructor_params {
    ($($version:literal),+) => {
            $(
            paste! {
                impl HasLotusJson for fil_actor_miner_state::[<v $version>]::MinerConstructorParams {
                    type LotusJson = MinerConstructorParamsLotusJson;

                    #[cfg(test)]
                    fn snapshots() -> Vec<(serde_json::Value, Self)> {
                        vec![
                            (
                                json!({
                                    "Owner": "f01234",
                                    "Worker": "f01235",
                                    "ControlAddrs": ["f01236", "f01237"],
                                    "WindowPoStProofType": 1,
                                    "PeerId": "AQ==",
                                    "Multiaddrs": ["Ag==", "Aw=="],
                                }),
                                Self {
                                    owner: Address::new_id(1234).into(),
                                    worker: Address::new_id(1235).into(),
                                    control_addresses: vec![Address::new_id(1236).into(), Address::new_id(1237).into()],
                                    window_post_proof_type: fvm_shared4::sector::RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
                                    peer_id: vec![1],
                                    multi_addresses: vec![],
                                },
                            ),
                        ]
                    }

                    fn into_lotus_json(self) -> Self::LotusJson {
                        MinerConstructorParamsLotusJson {
                            owner_addr: self.owner.into(),
                            worker_addr: self.worker.into(),
                            control_addrs: self.control_addresses.into_iter().map(|a| a.into()).collect(),
                            window_post_proof_type: self.window_post_proof_type.into(),
                            peer_id: self.peer_id,
                            multiaddrs: self.multi_addresses.into_iter().map(|addr| addr.0).collect(),
                        }
                    }

                    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                        Self {
                            owner: lotus_json.owner_addr.into(),
                            worker: lotus_json.worker_addr.into(),
                            control_addresses: lotus_json.control_addrs
                                .into_iter()
                                .map(|a| a.into())
                                .collect(),
                            window_post_proof_type: lotus_json.window_post_proof_type.into(),
                            peer_id: lotus_json.peer_id,
                            multi_addresses: lotus_json.multiaddrs.into_iter().map(BytesDe).collect(),
                        }
                    }
                }
            }
        )+
    };
}

impl_lotus_json_for_miner_constructor_params!(12, 13, 14, 15, 16);
