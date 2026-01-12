// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::HasLotusJson;
use crate::rpc::types::ClaimLotusJson;
use crate::shim::actors::verifreg::Claim;

impl HasLotusJson for Claim {
    type LotusJson = ClaimLotusJson;
    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }
    fn into_lotus_json(self) -> Self::LotusJson {
        ClaimLotusJson {
            size: self.size,
            sector: self.sector,
            data: self.data,
            client: self.client,
            provider: self.provider,
            term_max: self.term_max,
            term_min: self.term_min,
            term_start: self.term_start,
        }
    }
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        Claim {
            size: lotus_json.size,
            sector: lotus_json.sector,
            data: lotus_json.data,
            client: lotus_json.client,
            provider: lotus_json.provider,
            term_max: lotus_json.term_max,
            term_min: lotus_json.term_min,
            term_start: lotus_json.term_start,
        }
    }
}
