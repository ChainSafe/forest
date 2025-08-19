// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use anyhow::Result;
use cid::Cid;

macro_rules! register_market_versions_8_to_9 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{Method};

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                // (Method::AddBalance, AddBalanceParams),
                // (Method::WithdrawBalance, WithdrawBalanceParams),
                // (Method::PublishStorageDeals, PublishStorageDealsParams),
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, empty),
                (Method::CronTick, empty),
            ]
        );
    }};
}

macro_rules! register_market_versions_10_to_11 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{Method, PublishStorageDealsParams, WithdrawBalanceParams};

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                // (Method::AddBalance, AddBalanceParams),
                // (Method::WithdrawBalance, WithdrawBalanceParams),
                // (Method::PublishStorageDeals, PublishStorageDealsParams),
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [(Method::Constructor, empty), (Method::CronTick, empty),]
        );
    }};
}

macro_rules! register_market_versions_12 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            AddBalanceParams, Method, PublishStorageDealsParams, WithdrawBalanceParams,
        };

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::AddBalance, AddBalanceParams),
                // (Method::WithdrawBalance, WithdrawBalanceParams),
                // (Method::PublishStorageDeals, PublishStorageDealsParams),
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [(Method::Constructor, empty), (Method::CronTick, empty),]
        );
    }};
}

macro_rules! register_market_versions_13_to_16 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            AddBalanceParams, Method, PublishStorageDealsParams, WithdrawBalanceParams,
        };

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::AddBalance, AddBalanceParams),
                (Method::WithdrawBalance, WithdrawBalanceParams),
                (Method::PublishStorageDeals, PublishStorageDealsParams)
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [(Method::Constructor, empty), (Method::CronTick, empty),]
        );
    }};
}

pub(crate) fn register_actor_methods(registry: &mut MethodRegistry, cid: Cid, version: u64) {
    match version {
        8 => register_market_versions_8_to_9!(registry, cid, fil_actor_market_state::v8),
        9 => register_market_versions_8_to_9!(registry, cid, fil_actor_market_state::v9),
        10 => register_market_versions_10_to_11!(registry, cid, fil_actor_market_state::v10),
        11 => register_market_versions_10_to_11!(registry, cid, fil_actor_market_state::v11),
        12 => register_market_versions_12!(registry, cid, fil_actor_market_state::v12),
        13 => register_market_versions_13_to_16!(registry, cid, fil_actor_market_state::v13),
        14 => register_market_versions_13_to_16!(registry, cid, fil_actor_market_state::v14),
        15 => register_market_versions_13_to_16!(registry, cid, fil_actor_market_state::v15),
        16 => register_market_versions_13_to_16!(registry, cid, fil_actor_market_state::v16),
        _ => {}
    }
}
