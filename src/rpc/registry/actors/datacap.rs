// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use anyhow::Result;
use cid::Cid;

macro_rules! register_datacap_v9 {
    ($registry:expr, $code_cid:expr) => {{
        use fil_actor_datacap_state::v9::{DestroyParams, Method, MintParams};
        register_actor_methods!(
            $registry,
            $code_cid,
            [(Method::Mint, MintParams), (Method::Destroy, DestroyParams),]
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
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::MintExported, MintParams),
                (Method::DestroyExported, DestroyParams),
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
        use $state_version::{BalanceParams, ConstructorParams, DestroyParams, Method, MintParams};
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, ConstructorParams),
                (Method::MintExported, MintParams),
                (Method::DestroyExported, DestroyParams),
                (Method::BalanceExported, BalanceParams),
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
    version: u64,
) {
    match version {
        9 => register_datacap_v9!(registry, cid),
        10 => register_datacap_v10!(registry, cid),
        11 => register_datacap_version!(registry, cid, fil_actor_datacap_state::v11),
        12 => register_datacap_version!(registry, cid, fil_actor_datacap_state::v12),
        13 => register_datacap_version!(registry, cid, fil_actor_datacap_state::v13),
        14 => register_datacap_version!(registry, cid, fil_actor_datacap_state::v14),
        15 => register_datacap_version!(registry, cid, fil_actor_datacap_state::v15),
        16 => register_datacap_version!(registry, cid, fil_actor_datacap_state::v16),
        _ => {}
    }
}
