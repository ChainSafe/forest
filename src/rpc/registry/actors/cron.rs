// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use cid::Cid;
use fil_actors_shared::actor_versions::ActorVersion;

macro_rules! register_cron_version {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{ConstructorParams, Method};

        register_actor_methods!(
            $registry,
            $code_cid,
            [(Method::Constructor, ConstructorParams),]
        );

        // Register methods with empty params
        register_actor_methods!($registry, $code_cid, [(Method::EpochTick, empty),]);
    }};
}

pub(crate) fn register_actor_methods(
    registry: &mut MethodRegistry,
    cid: Cid,
    version: ActorVersion,
) {
    match version {
        ActorVersion::V8 => register_cron_version!(registry, cid, fil_actor_cron_state::v8),
        ActorVersion::V9 => register_cron_version!(registry, cid, fil_actor_cron_state::v9),
        ActorVersion::V10 => register_cron_version!(registry, cid, fil_actor_cron_state::v10),
        ActorVersion::V11 => register_cron_version!(registry, cid, fil_actor_cron_state::v11),
        ActorVersion::V12 => register_cron_version!(registry, cid, fil_actor_cron_state::v12),
        ActorVersion::V13 => register_cron_version!(registry, cid, fil_actor_cron_state::v13),
        ActorVersion::V14 => register_cron_version!(registry, cid, fil_actor_cron_state::v14),
        ActorVersion::V15 => register_cron_version!(registry, cid, fil_actor_cron_state::v15),
        ActorVersion::V16 => register_cron_version!(registry, cid, fil_actor_cron_state::v16),
        ActorVersion::V17 => register_cron_version!(registry, cid, fil_actor_cron_state::v17),
    }
}
