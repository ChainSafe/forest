// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use cid::Cid;
use fil_actors_shared::actor_versions::ActorVersion;

// Payment channel methods are consistent across all versions V8-V16
macro_rules! register_payment_channel_methods {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{ConstructorParams, Method, UpdateChannelStateParams};

        // Register methods with parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, ConstructorParams),
                (Method::UpdateChannelState, UpdateChannelStateParams),
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [(Method::Settle, empty), (Method::Collect, empty),]
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
            register_payment_channel_methods!(registry, cid, fil_actor_paych_state::v8)
        }
        ActorVersion::V9 => {
            register_payment_channel_methods!(registry, cid, fil_actor_paych_state::v9)
        }
        ActorVersion::V10 => {
            register_payment_channel_methods!(registry, cid, fil_actor_paych_state::v10)
        }
        ActorVersion::V11 => {
            register_payment_channel_methods!(registry, cid, fil_actor_paych_state::v11)
        }
        ActorVersion::V12 => {
            register_payment_channel_methods!(registry, cid, fil_actor_paych_state::v12)
        }
        ActorVersion::V13 => {
            register_payment_channel_methods!(registry, cid, fil_actor_paych_state::v13)
        }
        ActorVersion::V14 => {
            register_payment_channel_methods!(registry, cid, fil_actor_paych_state::v14)
        }
        ActorVersion::V15 => {
            register_payment_channel_methods!(registry, cid, fil_actor_paych_state::v15)
        }
        ActorVersion::V16 => {
            register_payment_channel_methods!(registry, cid, fil_actor_paych_state::v16)
        }
    }
}
