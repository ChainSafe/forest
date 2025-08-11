// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use anyhow::Result;
use cid::Cid;

// Macro for versions 8-10 that only have Exec method
macro_rules! register_market_versions_8_to_10 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{ConstructorParams, ExecParams, Method};

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, ConstructorParams),
                (Method::Exec, ExecParams),
            ]
        );
    }};
}

// Macro for versions 11-16 that have Exec4
macro_rules! register_market_versions_11_to_16 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{ConstructorParams, Exec4Params, ExecParams, Method};

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, ConstructorParams),
                (Method::Exec, ExecParams),
                (Method::Exec4, Exec4Params)
            ]
        );
    }};
}

pub(crate) fn register_actor_methods(registry: &mut MethodRegistry, cid: Cid, version: u64) {
    match version {
        8 => register_market_versions_8_to_10!(registry, cid, fil_actor_init_state::v8),
        9 => register_market_versions_8_to_10!(registry, cid, fil_actor_init_state::v9),
        10 => register_market_versions_8_to_10!(registry, cid, fil_actor_init_state::v10),
        11 => register_market_versions_11_to_16!(registry, cid, fil_actor_init_state::v11),
        12 => register_market_versions_11_to_16!(registry, cid, fil_actor_init_state::v12),
        13 => register_market_versions_11_to_16!(registry, cid, fil_actor_init_state::v13),
        14 => register_market_versions_11_to_16!(registry, cid, fil_actor_init_state::v14),
        15 => register_market_versions_11_to_16!(registry, cid, fil_actor_init_state::v15),
        16 => register_market_versions_11_to_16!(registry, cid, fil_actor_init_state::v16),
        _ => {}
    }
}
