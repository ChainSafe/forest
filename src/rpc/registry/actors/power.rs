// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use anyhow::Result;
use cid::Cid;

// Macro for versions 8-9 that have limited methods
macro_rules! register_power_versions_8_to_9 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            CreateMinerParams, EnrollCronEventParams, Method, UpdateClaimedPowerParams,
        };

        // Register methods with parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::CreateMiner, CreateMinerParams),
                (Method::UpdateClaimedPower, UpdateClaimedPowerParams),
                (Method::EnrollCronEvent, EnrollCronEventParams),
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, empty),
                (Method::OnEpochTickEnd, empty),
                (Method::CurrentTotalPower, empty),
            ]
        );
    }};
}

// Macro for versions 10-15 that have most methods but not MinerPowerParams
macro_rules! register_power_versions_10_to_15 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            CreateMinerParams, EnrollCronEventParams, Method, MinerRawPowerParams,
            UpdateClaimedPowerParams, UpdatePledgeTotalParams,
        };

        // Register methods with parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::CreateMiner, CreateMinerParams),
                (Method::UpdateClaimedPower, UpdateClaimedPowerParams),
                (Method::EnrollCronEvent, EnrollCronEventParams),
                (Method::UpdatePledgeTotal, UpdatePledgeTotalParams),
                (Method::CreateMinerExported, CreateMinerParams),
                (Method::MinerRawPowerExported, MinerRawPowerParams),
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, empty),
                (Method::OnEpochTickEnd, empty),
                (Method::CurrentTotalPower, empty),
                (Method::NetworkRawPowerExported, empty),
                (Method::MinerCountExported, empty),
                (Method::MinerConsensusCountExported, empty),
            ]
        );
    }};
}

// Macro for version 16 that has all methods
macro_rules! register_power_version_16 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            CreateMinerParams, EnrollCronEventParams, Method, MinerPowerParams,
            MinerRawPowerParams, UpdateClaimedPowerParams, UpdatePledgeTotalParams,
        };

        // Register methods with parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::CreateMiner, CreateMinerParams),
                (Method::UpdateClaimedPower, UpdateClaimedPowerParams),
                (Method::EnrollCronEvent, EnrollCronEventParams),
                (Method::UpdatePledgeTotal, UpdatePledgeTotalParams),
                (Method::CreateMinerExported, CreateMinerParams),
                (Method::MinerRawPowerExported, MinerRawPowerParams),
                (Method::MinerPowerExported, MinerPowerParams),
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, empty),
                (Method::OnEpochTickEnd, empty),
                (Method::CurrentTotalPower, empty),
                (Method::NetworkRawPowerExported, empty),
                (Method::MinerCountExported, empty),
                (Method::MinerConsensusCountExported, empty),
            ]
        );
    }};
}

pub(crate) fn register_actor_methods(registry: &mut MethodRegistry, cid: Cid, version: u64) {
    match version {
        8 => register_power_versions_8_to_9!(registry, cid, fil_actor_power_state::v8),
        9 => register_power_versions_8_to_9!(registry, cid, fil_actor_power_state::v9),
        10 => register_power_versions_10_to_15!(registry, cid, fil_actor_power_state::v10),
        11 => register_power_versions_10_to_15!(registry, cid, fil_actor_power_state::v11),
        12 => register_power_versions_10_to_15!(registry, cid, fil_actor_power_state::v12),
        13 => register_power_versions_10_to_15!(registry, cid, fil_actor_power_state::v13),
        14 => register_power_versions_10_to_15!(registry, cid, fil_actor_power_state::v14),
        15 => register_power_versions_10_to_15!(registry, cid, fil_actor_power_state::v15),
        16 => register_power_version_16!(registry, cid, fil_actor_power_state::v16),
        _ => {}
    }
}
