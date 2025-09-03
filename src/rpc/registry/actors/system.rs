// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use cid::Cid;
use fil_actors_shared::actor_versions::ActorVersion;

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

pub(crate) fn register_actor_methods(
    registry: &mut MethodRegistry,
    cid: Cid,
    version: ActorVersion,
) {
    match version {
        ActorVersion::V8 => register_system_version!(registry, cid, fil_actor_system_state::v8),
        ActorVersion::V9 => register_system_version!(registry, cid, fil_actor_system_state::v9),
        ActorVersion::V10 => register_system_version!(registry, cid, fil_actor_system_state::v10),
        ActorVersion::V11 => register_system_version!(registry, cid, fil_actor_system_state::v11),
        ActorVersion::V12 => register_system_version!(registry, cid, fil_actor_system_state::v12),
        ActorVersion::V13 => register_system_version!(registry, cid, fil_actor_system_state::v13),
        ActorVersion::V14 => register_system_version!(registry, cid, fil_actor_system_state::v14),
        ActorVersion::V15 => register_system_version!(registry, cid, fil_actor_system_state::v15),
        ActorVersion::V16 => register_system_version!(registry, cid, fil_actor_system_state::v16),
    }
}
