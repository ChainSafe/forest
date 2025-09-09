// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::address::Address;
use crate::shim::message::MethodNum;
use cid::Cid;
use fil_actors_shared::actor_versions::ActorVersion;

macro_rules! register_datacap_v9 {
    ($registry:expr, $code_cid:expr) => {{
        use fil_actor_datacap_state::v9::{DestroyParams, Method, MintParams};
        use fil_actors_shared::frc46_token::token::types::{
            BurnFromParams, BurnParams, DecreaseAllowanceParams, GetAllowanceParams,
            IncreaseAllowanceParams, RevokeAllowanceParams, TransferFromParams, TransferParams,
        };
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, Address),
                (Method::BalanceOf, Address),
                (Method::Mint, MintParams),
                (Method::Destroy, DestroyParams),
                (Method::Transfer, TransferParams),
                (Method::TransferFrom, TransferFromParams),
                (Method::IncreaseAllowance, IncreaseAllowanceParams),
                (Method::DecreaseAllowance, DecreaseAllowanceParams),
                (Method::RevokeAllowance, RevokeAllowanceParams),
                (Method::Burn, BurnParams),
                (Method::BurnFrom, BurnFromParams),
                (Method::Allowance, GetAllowanceParams),
            ]
        );

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Name, empty),
                (Method::Symbol, empty),
                (Method::TotalSupply, empty),
            ]
        );
    }};
}

macro_rules! register_datacap_v10 {
    ($registry:expr, $code_cid:expr) => {{
        use fil_actor_datacap_state::v10::{DestroyParams, Method, MintParams};
        use fil_actors_shared::frc46_token::token::types::{
            BurnFromParams, BurnParams, DecreaseAllowanceParams, GetAllowanceParams,
            IncreaseAllowanceParams, RevokeAllowanceParams, TransferFromParams, TransferParams,
        };
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, Address),
                (Method::BalanceExported, Address),
                (Method::MintExported, MintParams),
                (Method::DestroyExported, DestroyParams),
                (Method::TransferExported, TransferParams),
                (Method::TransferFromExported, TransferFromParams),
                (Method::IncreaseAllowanceExported, IncreaseAllowanceParams),
                (Method::DecreaseAllowanceExported, DecreaseAllowanceParams),
                (Method::RevokeAllowanceExported, RevokeAllowanceParams),
                (Method::BurnExported, BurnParams),
                (Method::BurnFromExported, BurnFromParams),
                (Method::AllowanceExported, GetAllowanceParams),
            ]
        );

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::NameExported, empty),
                (Method::SymbolExported, empty),
                (Method::TotalSupplyExported, empty),
                (Method::GranularityExported, empty)
            ]
        );
    }};
}

macro_rules! register_datacap_version {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use fil_actors_shared::frc46_token::token::types::{
            BurnFromParams, BurnParams, DecreaseAllowanceParams, GetAllowanceParams,
            IncreaseAllowanceParams, RevokeAllowanceParams, TransferFromParams, TransferParams,
        };
        use $state_version::{BalanceParams, ConstructorParams, DestroyParams, Method, MintParams};
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, ConstructorParams),
                (Method::MintExported, MintParams),
                (Method::DestroyExported, DestroyParams),
                (Method::BalanceExported, BalanceParams),
                (Method::TransferExported, TransferParams),
                (Method::TransferFromExported, TransferFromParams),
                (Method::IncreaseAllowanceExported, IncreaseAllowanceParams),
                (Method::DecreaseAllowanceExported, DecreaseAllowanceParams),
                (Method::RevokeAllowanceExported, RevokeAllowanceParams),
                (Method::BurnExported, BurnParams),
                (Method::BurnFromExported, BurnFromParams),
                (Method::AllowanceExported, GetAllowanceParams),
            ]
        );

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::NameExported, empty),
                (Method::SymbolExported, empty),
                (Method::TotalSupplyExported, empty),
                (Method::GranularityExported, empty)
            ]
        );
    }};
}

pub(crate) fn register_datacap_actor_methods(
    registry: &mut MethodRegistry,
    cid: Cid,
    version: ActorVersion,
) {
    match version {
        ActorVersion::V8 => {}
        ActorVersion::V9 => register_datacap_v9!(registry, cid),
        ActorVersion::V10 => register_datacap_v10!(registry, cid),
        ActorVersion::V11 => register_datacap_version!(registry, cid, fil_actor_datacap_state::v11),
        ActorVersion::V12 => register_datacap_version!(registry, cid, fil_actor_datacap_state::v12),
        ActorVersion::V13 => register_datacap_version!(registry, cid, fil_actor_datacap_state::v13),
        ActorVersion::V14 => register_datacap_version!(registry, cid, fil_actor_datacap_state::v14),
        ActorVersion::V15 => register_datacap_version!(registry, cid, fil_actor_datacap_state::v15),
        ActorVersion::V16 => register_datacap_version!(registry, cid, fil_actor_datacap_state::v16),
        ActorVersion::V17 => register_datacap_version!(registry, cid, fil_actor_datacap_state::v17),
    }
}
