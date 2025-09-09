// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use cid::Cid;
use fil_actors_shared::actor_versions::ActorVersion;

macro_rules! register_eam_reg_version {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{Create2Params, CreateExternalParams, CreateParams, Method};

        // Constructor has no parameters
        register_actor_methods!($registry, $code_cid, [(Method::Constructor, empty),]);

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Create, CreateParams),
                (Method::Create2, Create2Params),
                (Method::CreateExternal, CreateExternalParams),
            ]
        );
    }};
}

pub(crate) fn register_actor_methods(registry: &mut MethodRegistry, cid: Cid, version: ActorVersion) {
    match version {
        ActorVersion::V10 => register_eam_reg_version!(registry, cid, fil_actor_eam_state::v10),
        ActorVersion::V11 => register_eam_reg_version!(registry, cid, fil_actor_eam_state::v11),
        ActorVersion::V12 => register_eam_reg_version!(registry, cid, fil_actor_eam_state::v12),
        ActorVersion::V13 => register_eam_reg_version!(registry, cid, fil_actor_eam_state::v13),
        ActorVersion::V14 => register_eam_reg_version!(registry, cid, fil_actor_eam_state::v14),
        ActorVersion::V15 => register_eam_reg_version!(registry, cid, fil_actor_eam_state::v15),
        ActorVersion::V15 => register_eam_reg_version!(registry, cid, fil_actor_eam_state::v16),
        _ => {}
    }
}
