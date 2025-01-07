// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::verifreg::Allocation;
use ::cid::Cid;
use fvm_shared4::clock::ChainEpoch;
use fvm_shared4::piece::PaddedPieceSize;
use fvm_shared4::ActorID;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "Allocation")]
pub struct AllocationLotusJson {
    // The verified client which allocated the DataCap.
    pub client: ActorID,
    // The provider (miner actor) which may claim the allocation.
    pub provider: ActorID,
    // Identifier of the data to be committed.
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub data: Cid,
    // The (padded) size of data.
    #[schemars(with = "u64")]
    pub size: PaddedPieceSize,
    // The minimum duration which the provider must commit to storing the piece to avoid
    // early-termination penalties (epochs).
    pub term_min: ChainEpoch,
    // The maximum period for which a provider can earn quality-adjusted power
    // for the piece (epochs).
    pub term_max: ChainEpoch,
    // The latest epoch by which a provider must commit data before the allocation expires.
    pub expiration: ChainEpoch,
}

impl HasLotusJson for Allocation {
    type LotusJson = AllocationLotusJson;
    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json! {{
                "TermMin": 0,
                "TermMax": 0,
                "Provider": 0,
                "Client": 0,
                "Expiration": 0,
                "Size": 0,
                "Data": {"/":"baeaaaaa"},
            }},
            Allocation {
                term_min: 0,
                term_max: 0,
                provider: 0,
                client: 0,
                expiration: 0,
                size: PaddedPieceSize(0),
                data: Cid::default(),
            },
        )]
    }
    fn into_lotus_json(self) -> Self::LotusJson {
        AllocationLotusJson {
            client: self.client,
            provider: self.provider,
            data: self.data,
            size: self.size,
            term_min: self.term_min,
            term_max: self.term_max,
            expiration: self.expiration,
        }
    }
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Allocation {
            client: lotus_json.client,
            provider: lotus_json.provider,
            data: lotus_json.data,
            size: lotus_json.size,
            term_min: lotus_json.term_min,
            term_max: lotus_json.term_max,
            expiration: lotus_json.expiration,
        }
    }
}

#[test]
fn snapshots() {
    assert_all_snapshots::<Allocation>();
}
