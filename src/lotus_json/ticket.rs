// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::{Ticket, VRFProof};

use super::*;

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "Ticket")]
pub struct TicketLotusJson {
    #[schemars(with = "LotusJson<VRFProof>")]
    #[serde(with = "crate::lotus_json")]
    v_r_f_proof: VRFProof,
}

impl HasLotusJson for Ticket {
    type LotusJson = TicketLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![(
            json!({"VRFProof": "aGVsbG8gd29ybGQh"}),
            Ticket {
                vrfproof: crate::blocks::VRFProof(Vec::from_iter(*b"hello world!")),
            },
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let Self { vrfproof } = self;
        Self::LotusJson {
            v_r_f_proof: vrfproof,
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson { v_r_f_proof } = lotus_json;
        Self {
            vrfproof: v_r_f_proof,
        }
    }
}
