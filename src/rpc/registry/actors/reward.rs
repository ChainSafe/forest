// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use anyhow::Result;
use cid::Cid;

macro_rules! register_reward_version_11_to_16 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            AwardBlockRewardParams, ConstructorParams, Method, UpdateNetworkKPIParams,
        };

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, ConstructorParams),
                (Method::AwardBlockReward, AwardBlockRewardParams),
                (Method::UpdateNetworkKPI, UpdateNetworkKPIParams),
            ]
        );

        // Register methods without parameters
        register_actor_methods!($registry, $code_cid, [(Method::ThisEpochReward, empty)]);
    }};
}

macro_rules! register_reward_version_8_to_10 {
    ($registry:expr, $code_cid:expr, $state_version:path, $fvm_shared_version:path) => {{
        use $state_version::{AwardBlockRewardParams, Method};
        use $fvm_shared_version::{bigint::bigint_ser::BigIntDe};

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, Option<BigIntDe>),
                (Method::AwardBlockReward, AwardBlockRewardParams),
                (Method::UpdateNetworkKPI, Option<BigIntDe>),
            ]
        );

        // Register methods without parameters
        register_actor_methods!($registry, $code_cid, [(Method::ThisEpochReward, empty)]);
    }};
}

pub(crate) fn register_actor_methods(registry: &mut MethodRegistry, cid: Cid, version: u64) {
    match version {
        8 => {
            register_reward_version_8_to_10!(registry, cid, fil_actor_reward_state::v8, fvm_shared2)
        }
        9 => {
            register_reward_version_8_to_10!(registry, cid, fil_actor_reward_state::v9, fvm_shared2)
        }
        10 => {
            register_reward_version_8_to_10!(
                registry,
                cid,
                fil_actor_reward_state::v10,
                fvm_shared3
            )
        }
        11 => register_reward_version_11_to_16!(registry, cid, fil_actor_reward_state::v11),
        12 => register_reward_version_11_to_16!(registry, cid, fil_actor_reward_state::v12),
        13 => register_reward_version_11_to_16!(registry, cid, fil_actor_reward_state::v13),
        14 => register_reward_version_11_to_16!(registry, cid, fil_actor_reward_state::v14),
        15 => register_reward_version_11_to_16!(registry, cid, fil_actor_reward_state::v15),
        16 => register_reward_version_11_to_16!(registry, cid, fil_actor_reward_state::v16),
        _ => {}
    }
}
