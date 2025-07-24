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

pub(crate) fn register_actor_methods(registry: &mut MethodRegistry, cid: Cid) {
    register_system_version!(registry, cid, fil_actor_system_state::v8);
    register_system_version!(registry, cid, fil_actor_system_state::v9);
    register_system_version!(registry, cid, fil_actor_system_state::v10);
    register_system_version!(registry, cid, fil_actor_system_state::v11);
    register_system_version!(registry, cid, fil_actor_system_state::v12);
    register_system_version!(registry, cid, fil_actor_system_state::v13);
    register_system_version!(registry, cid, fil_actor_system_state::v14);
    register_system_version!(registry, cid, fil_actor_system_state::v15);
    register_system_version!(registry, cid, fil_actor_system_state::v16);
}
