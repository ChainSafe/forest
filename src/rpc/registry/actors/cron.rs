// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use anyhow::Result;
use cid::Cid;
use paste::paste;

macro_rules! register_cron_versions_8_to_16 {
    ($registry:expr, $code_cid:expr, $($version:literal),+) => {
        $(
            paste! {
                {
                    use fil_actor_cron_state::[<v $version>]::{ConstructorParams, Method};

                    register_actor_methods!(
                        $registry,
                        $code_cid,
                        [
                            (Method::Constructor, ConstructorParams),
                        ]
                    );
                }
            }
        )+
    };
}

pub(crate) fn register_actor_methods(registry: &mut MethodRegistry, cid: Cid) {
    register_cron_versions_8_to_16!(registry, cid, 8, 9, 10, 11, 12, 13, 14, 15, 16);
}
