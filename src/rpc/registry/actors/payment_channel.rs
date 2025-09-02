// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use cid::Cid;

macro_rules! register_payment_channel_reg_versions {
    ($registry:expr, $code_cid:expr, $version:literal) => {{
        paste::paste!{
            use fil_actor_paych_state::[<v $version>]::{ConstructorParams, Method, UpdateChannelStateParams};
        }

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
    macro_rules! register_versions {
        ($($version:literal),+) => {{
            match version {
                $(
                    $version => register_payment_channel_reg_versions!(registry, cid, $version),
                )+
                _ => {}
            }
        }};
    }
    register_versions!(8, 9, 10, 11, 12, 13, 14, 15, 16)
}
