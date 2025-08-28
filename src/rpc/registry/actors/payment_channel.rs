// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use cid::Cid;

macro_rules! register_payment_channel_reg_versions {
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

pub(crate) fn register_actor_methods(registry: &mut MethodRegistry, cid: Cid, version: u64) {
    match version {
        8 => register_payment_channel_reg_versions!(registry, cid, fil_actor_paych_state::v8),
        9 => register_payment_channel_reg_versions!(registry, cid, fil_actor_paych_state::v9),
        10 => register_payment_channel_reg_versions!(registry, cid, fil_actor_paych_state::v10),
        11 => register_payment_channel_reg_versions!(registry, cid, fil_actor_paych_state::v11),
        12 => register_payment_channel_reg_versions!(registry, cid, fil_actor_paych_state::v12),
        13 => register_payment_channel_reg_versions!(registry, cid, fil_actor_paych_state::v13),
        14 => register_payment_channel_reg_versions!(registry, cid, fil_actor_paych_state::v14),
        15 => register_payment_channel_reg_versions!(registry, cid, fil_actor_paych_state::v15),
        16 => register_payment_channel_reg_versions!(registry, cid, fil_actor_paych_state::v16),
        _ => {}
    }
}
