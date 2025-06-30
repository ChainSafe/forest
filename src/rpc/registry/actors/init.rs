// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use anyhow::Result;
use cid::Cid;
use paste::paste;

// Macro for versions 8-10 that only have Exec method
macro_rules! register_init_versions_8_to_10 {
    ($registry:expr, $code_cid:expr, $($version:literal),+) => {
        $(
            paste! {
                {
                    use fil_actor_init_state::[<v $version>]::{ConstructorParams, ExecParams, Method};

                    register_actor_methods!(
                        $registry,
                        $code_cid,
                        [
                            (Method::Constructor, ConstructorParams),
                            (Method::Exec, ExecParams),
                        ]
                    );
                }
            }
        )+
    };
}

// Macro for versions 11-16 that have Exec4
macro_rules! register_init_versions_11_to_16 {
    ($registry:expr, $code_cid:expr, $($version:literal),+) => {
        $(
            paste! {
                {
                    use fil_actor_init_state::[<v $version>]::{ConstructorParams, Exec4Params, ExecParams, Method};

                    register_actor_methods!(
                        $registry,
                        $code_cid,
                        [
                            (Method::Constructor, ConstructorParams),
                            (Method::Exec, ExecParams),
                            (Method::Exec4, Exec4Params)
                        ]
                    );
                }
            }
        )+
    };
}

pub(crate) fn register_actor_methods(registry: &mut MethodRegistry, cid: Cid) {
    register_init_versions_11_to_16!(registry, cid, 11, 12, 13, 14, 15, 16);
    // Version 10 has Exec4, but it's not present in the `fil-actor-init-state` crate.
    register_init_versions_8_to_10!(registry, cid, 8, 9, 10);
}
