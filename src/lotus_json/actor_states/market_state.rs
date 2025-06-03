// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use super::*;
use crate::shim::actors::market::State;
use crate::shim::{clock::ChainEpoch, econ::TokenAmount};
use ::cid::Cid;
use fvm_shared4::deal::DealID;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
#[schemars(rename = "MarketState")]
pub struct MarketStateLotusJson {
    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub proposals: Cid,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub states: Cid,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub pending_proposals: Cid,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub escrow_table: Cid,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub locked_table: Cid,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json", rename = "NextID")]
    pub next_id: DealID,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json")]
    pub deal_ops_by_epoch: Cid,

    pub last_cron: ChainEpoch,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub total_client_locked_collateral: TokenAmount,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub total_provider_locked_collateral: TokenAmount,

    #[schemars(with = "LotusJson<TokenAmount>")]
    #[serde(with = "crate::lotus_json")]
    pub total_client_storage_fee: TokenAmount,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json", skip_serializing_if = "Option::is_none")]
    pub pending_deal_allocation_ids: Option<Cid>,

    #[schemars(with = "LotusJson<Cid>")]
    #[serde(with = "crate::lotus_json", skip_serializing_if = "Option::is_none")]
    pub provider_sectors: Option<Cid>,
}

// Define macros for field handling
macro_rules! common_market_state_fields {
    ($state:expr) => {{
        MarketStateLotusJson {
            proposals: $state.proposals,
            states: $state.states,
            pending_proposals: $state.pending_proposals,
            escrow_table: $state.escrow_table,
            locked_table: $state.locked_table,
            next_id: $state.next_id,
            deal_ops_by_epoch: $state.deal_ops_by_epoch,
            last_cron: $state.last_cron,
            total_client_locked_collateral: $state.total_client_locked_collateral.into(),
            total_provider_locked_collateral: $state.total_provider_locked_collateral.into(),
            total_client_storage_fee: $state.total_client_storage_fee.into(),
            pending_deal_allocation_ids: None,
            provider_sectors: None,
        }
    }};
}

// A macro that implements the field handling for v8
macro_rules! v8_market_state_fields {
    ($state:expr) => {{
        MarketStateLotusJson {
            ..common_market_state_fields!($state)
        }
    }};
}

// A macro that implements the field handling for v9 to v12
macro_rules! v9_to_v12_market_state_fields {
    ($state:expr) => {{
        MarketStateLotusJson {
            pending_deal_allocation_ids: Some($state.pending_deal_allocation_ids),
            ..common_market_state_fields!($state)
        }
    }};
}

// A macro that implements the field handling for v13 to v16
macro_rules! v13_plus_market_state_fields {
    ($state:expr) => {{
        MarketStateLotusJson {
            pending_deal_allocation_ids: Some($state.pending_deal_allocation_ids),
            provider_sectors: Some($state.provider_sectors),
            ..common_market_state_fields!($state)
        }
    }};
}

// A macro that implements the trait method for each version
macro_rules! implement_state_versions {
    (
        $(
            $handler:ident for [ $( $version:ident ),+ ]
        );* $(;)?
    ) => {
        impl HasLotusJson for State {
            type LotusJson = MarketStateLotusJson;

            #[cfg(test)]
            fn snapshots() -> Vec<(serde_json::Value, Self)> {
                vec![(
                    json!({
                        "Proposals": {"/":"baeaaaaa"},
                        "States": {"/":"baeaaaaa"},
                        "PendingProposals": {"/":"baeaaaaa"},
                        "EscrowTable": {"/":"baeaaaaa"},
                        "LockedTable": {"/":"baeaaaaa"},
                        "NextID": 0,
                        "DealOpsByEpoch": {"/":"baeaaaaa"},
                        "LastCron": 0,
                        "TotalClientLockedCollateral": "0",
                        "TotalProviderLockedCollateral": "0",
                        "TotalClientStorageFee": "0",
                        "PendingDealAllocationIDs": {"/":"baeaaaaa"},
                        "ProviderSectors": {"/":"baeaaaaa"}
                    }),
                    State::V16(fil_actor_market_state::v16::State {
                        proposals: Default::default(),
                        states: Default::default(),
                        pending_proposals: Default::default(),
                        escrow_table: Default::default(),
                        locked_table: Default::default(),
                        next_id: Default::default(),
                        deal_ops_by_epoch: Default::default(),
                        last_cron: Default::default(),
                        total_client_locked_collateral: Default::default(),
                        total_provider_locked_collateral: Default::default(),
                        total_client_storage_fee: Default::default(),
                        pending_deal_allocation_ids: Default::default(),
                        provider_sectors: Default::default(),
                    }),
                )]
            }

            fn into_lotus_json(self) -> Self::LotusJson {
                match self {
                    $(
                        $(
                            State::$version(state) => $handler!(state),
                        )+
                    )*
                    #[allow(unreachable_patterns)]
                    _ => panic!("Unhandled State variant in into_lotus_json"),

                }
            }

            fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                // Default to latest version (V16) when deserializing
                State::V16(fil_actor_market_state::v16::State {
                    proposals: lotus_json.proposals,
                    states: lotus_json.states,
                    pending_proposals: lotus_json.pending_proposals,
                    escrow_table: lotus_json.escrow_table,
                    locked_table: lotus_json.locked_table,
                    next_id: lotus_json.next_id,
                    deal_ops_by_epoch: lotus_json.deal_ops_by_epoch,
                    last_cron: lotus_json.last_cron,
                    total_client_locked_collateral: lotus_json.total_client_locked_collateral.into(),
                    total_provider_locked_collateral: lotus_json.total_provider_locked_collateral.into(),
                    total_client_storage_fee: lotus_json.total_client_storage_fee.into(),
                    pending_deal_allocation_ids: lotus_json.pending_deal_allocation_ids.unwrap_or_default(),
                    provider_sectors: lotus_json.provider_sectors.unwrap_or_default(),
                })
            }
        }
    };
}

// Invoke with very explicit syntax
implement_state_versions! {
    v8_market_state_fields for [V8];
    v9_to_v12_market_state_fields for [V9, V10, V11, V12];
    v13_plus_market_state_fields for [V13, V14, V15, V16];
}
