// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::address::Address;
use crate::shim::clock::ChainEpoch;
use crate::shim::crypto::Signature;
use crate::shim::econ::TokenAmount;
use crate::shim::piece::PaddedPieceSize;

use ::cid::Cid;
use fil_actor_market_state::v16::{ClientDealProposal, DealProposal};
use schemars::JsonSchema;
use serde::Deserialize;
use std::fmt::Debug;

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ClientDealProposalLotusJson {
    #[schemars(with = "LotusJson<DealProposal>")]
    #[serde(with = "crate::lotus_json")]
    pub proposal: DealProposal,
    #[schemars(with = "LotusJson<Signature>")]
    #[serde(with = "crate::lotus_json")]
    pub client_signature: Signature,
}

impl HasLotusJson for ClientDealProposal {
    type LotusJson = ClientDealProposalLotusJson;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        let Self {
            proposal,
            client_signature,
        } = self;
        Self::LotusJson {
            proposal,
            client_signature: todo!(), //client_signature.into(),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        let Self::LotusJson {
            proposal,
            client_signature,
        } = lotus_json;
        Self {
            proposal,
            client_signature: todo!(), //client_signature.into(),
        }
    }
}
