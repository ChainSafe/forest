// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::shim::actors::verifreg::State;
use crate::shim::address::Address;
use ::cid::Cid;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "VerifiedRegistryState")]
pub struct VerifiedRegistryStateLotusJson {
    #[schemars(with = "LotusJson<Address>")]
    #[serde(with = "crate::lotus_json")]
    pub root_key: Address,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub verifiers: Cid,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json", rename = "RemoveDataCapProposalIDs")]
    pub remove_data_cap_proposal_ids: Cid,

    #[schemars(with = "LotusJson<Option<Cid>>")]
    #[serde(with = "crate::lotus_json", skip_serializing_if = "Option::is_none")]
    pub verified_clients: Option<Cid>, // only available in verified reg state version 8

    #[schemars(with = "LotusJson<Option<Cid>>")]
    #[serde(with = "crate::lotus_json", skip_serializing_if = "Option::is_none")]
    pub allocations: Option<Cid>, // not available in verified reg state version 8

    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_allocation_id: Option<u64>, // not available in verified reg state version 8

    #[schemars(with = "LotusJson<Option<Cid>>")]
    #[serde(with = "crate::lotus_json", skip_serializing_if = "Option::is_none")]
    pub claims: Option<Cid>, // not available in verified reg state version 8
}

macro_rules! v8_verified_reg_state_fields {
    ($state:expr) => {{
        VerifiedRegistryStateLotusJson {
            root_key: $state.root_key.into(),
            verifiers: $state.verifiers,
            verified_clients: Some($state.verified_clients),
            remove_data_cap_proposal_ids: $state.remove_data_cap_proposal_ids,
            allocations: None,
            next_allocation_id: None,
            claims: None,
        }
    }};
}

macro_rules! v9_to_latest_verified_reg_state_fields {
    ($state:expr) => {{
        VerifiedRegistryStateLotusJson {
            root_key: $state.root_key.into(),
            verifiers: $state.verifiers,
            remove_data_cap_proposal_ids: $state.remove_data_cap_proposal_ids,
            allocations: Some($state.allocations),
            next_allocation_id: Some($state.next_allocation_id),
            claims: Some($state.claims),
            verified_clients: None,
        }
    }};
}

macro_rules! impl_verified_reg_state_lotus_json {
    (
        $(
             $handler:ident for [ $( $version:ident ),+ ]
        );* $(;)?
    ) => {
        impl HasLotusJson for State {
            type LotusJson = VerifiedRegistryStateLotusJson;

            #[cfg(test)]
            fn snapshots() -> Vec<(serde_json::Value, Self)> {
                todo!()
            }

            fn into_lotus_json(self) -> Self::LotusJson {
               match self {
                    $(
                        $(
                            State::$version(state) => $handler!(state),
                        )+
                    )*
                }
            }

            // Default V16
            fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                State::V16(fil_actor_verifreg_state::v16::State {
                    root_key: lotus_json.root_key.into(),
                    verifiers: lotus_json.verifiers,
                    remove_data_cap_proposal_ids: lotus_json.remove_data_cap_proposal_ids,
                    allocations: lotus_json.allocations.unwrap(),
                    next_allocation_id: lotus_json.next_allocation_id.unwrap(),
                    claims: lotus_json.claims.unwrap(),
                })
            }
        }
    };
}

impl_verified_reg_state_lotus_json! {
    v8_verified_reg_state_fields for [V8];
    v9_to_latest_verified_reg_state_fields for [V9, V10, V11, V12, V13, V14, V15, V16];
}
