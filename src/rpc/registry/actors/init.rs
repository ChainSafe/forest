// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use cid::Cid;
use fil_actors_shared::actor_versions::ActorVersion;

// Macro for versions 8-9 that only have Exec method
macro_rules! register_init_versions_8_to_9 {
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

// Macro for versions 10-16 that have Exec4
macro_rules! register_init_versions_10_to_16 {
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

pub(crate) fn register_actor_methods(
    registry: &mut MethodRegistry,
    cid: Cid,
    version: ActorVersion,
) {
    match version {
        ActorVersion::V8 => {
            register_init_versions_8_to_9!(registry, cid, fil_actor_init_state::v8)
        }
        ActorVersion::V9 => {
            register_init_versions_8_to_9!(registry, cid, fil_actor_init_state::v9)
        }
        ActorVersion::V10 => {
            register_init_versions_10_to_16!(registry, cid, fil_actor_init_state::v10)
        }
        ActorVersion::V11 => {
            register_init_versions_10_to_16!(registry, cid, fil_actor_init_state::v11)
        }
        ActorVersion::V12 => {
            register_init_versions_10_to_16!(registry, cid, fil_actor_init_state::v12)
        }
        ActorVersion::V13 => {
            register_init_versions_10_to_16!(registry, cid, fil_actor_init_state::v13)
        }
        ActorVersion::V14 => {
            register_init_versions_10_to_16!(registry, cid, fil_actor_init_state::v14)
        }
        ActorVersion::V15 => {
            register_init_versions_10_to_16!(registry, cid, fil_actor_init_state::v15)
        }
        ActorVersion::V16 => {
            register_init_versions_10_to_16!(registry, cid, fil_actor_init_state::v16)
        }
    }
}
