// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use cid::Cid;

macro_rules! register_system_version {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::*;

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, empty), // constructor method doesn't accept any kind of param
            ]
        );
    }};
}

pub(crate) fn register_actor_methods(registry: &mut MethodRegistry, cid: Cid, version: u64) {
    match version {
        8 => register_system_version!(registry, cid, fil_actor_system_state::v8),
        9 => register_system_version!(registry, cid, fil_actor_system_state::v9),
        10 => register_system_version!(registry, cid, fil_actor_system_state::v10),
        11 => register_system_version!(registry, cid, fil_actor_system_state::v11),
        12 => register_system_version!(registry, cid, fil_actor_system_state::v12),
        13 => register_system_version!(registry, cid, fil_actor_system_state::v13),
        14 => register_system_version!(registry, cid, fil_actor_system_state::v14),
        15 => register_system_version!(registry, cid, fil_actor_system_state::v15),
        16 => register_system_version!(registry, cid, fil_actor_system_state::v16),
        _ => {},
    }
}
