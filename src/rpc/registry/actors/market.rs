// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::address::Address;
use crate::shim::message::MethodNum;
use cid::Cid;
use fil_actors_shared::actor_versions::ActorVersion;

macro_rules! register_market_basic_methods {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            AddBalanceParams, Method, OnMinerSectorsTerminateParams, PublishStorageDealsParams,
            VerifyDealsForActivationParams, WithdrawBalanceParams,
        };

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::AddBalance, AddBalanceParams),
                (Method::WithdrawBalance, WithdrawBalanceParams),
                (Method::PublishStorageDeals, PublishStorageDealsParams),
                (
                    Method::VerifyDealsForActivation,
                    VerifyDealsForActivationParams
                ),
                (
                    Method::OnMinerSectorsTerminate,
                    OnMinerSectorsTerminateParams
                )
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

macro_rules! register_market_exported_methods_v10_onwards {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            AddBalanceParams, GetDealActivationParams, GetDealClientCollateralParams,
            GetDealClientParams, GetDealDataCommitmentParams, GetDealLabelParams,
            GetDealProviderCollateralParams, GetDealProviderParams, GetDealTermParams,
            GetDealTotalPriceParams, GetDealVerifiedParams, Method, PublishStorageDealsParams,
            WithdrawBalanceParams,
        };

        // Register exported methods
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::AddBalanceExported, AddBalanceParams),
                (Method::WithdrawBalanceExported, WithdrawBalanceParams),
                (
                    Method::PublishStorageDealsExported,
                    PublishStorageDealsParams
                ),
                (Method::GetBalanceExported, Address),
                (
                    Method::GetDealDataCommitmentExported,
                    GetDealDataCommitmentParams
                ),
                (Method::GetDealClientExported, GetDealClientParams),
                (Method::GetDealProviderExported, GetDealProviderParams),
                (Method::GetDealLabelExported, GetDealLabelParams),
                (Method::GetDealTermExported, GetDealTermParams),
                (Method::GetDealTotalPriceExported, GetDealTotalPriceParams),
                (
                    Method::GetDealClientCollateralExported,
                    GetDealClientCollateralParams
                ),
                (
                    Method::GetDealProviderCollateralExported,
                    GetDealProviderCollateralParams
                ),
                (Method::GetDealVerifiedExported, GetDealVerifiedParams),
                (Method::GetDealActivationExported, GetDealActivationParams),
            ]
        );
    }};
}

macro_rules! register_market_exported_methods_v13_onwards {
    ($registry:expr, $code_cid:expr, $market_state_version:path) => {{
        use $market_state_version::{
            GetDealSectorParams, Method, SettleDealPaymentsParams,
            ext::miner::SectorContentChangedParams,
        };

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::GetDealSectorExported, GetDealSectorParams),
                (Method::SettleDealPaymentsExported, SettleDealPaymentsParams),
                (
                    Method::SectorContentChangedExported,
                    SectorContentChangedParams
                )
            ]
        );
    }};
}

macro_rules! register_market_versions_8_to_9 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        register_market_basic_methods!($registry, $code_cid, $state_version);

        use $state_version::{ActivateDealsParams, ComputeDataCommitmentParams, Method};

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::ActivateDeals, ActivateDealsParams),
                (Method::ComputeDataCommitment, ComputeDataCommitmentParams)
            ]
        );
    }};
}

macro_rules! register_market_versions_10_to_11 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        register_market_basic_methods!($registry, $code_cid, $state_version);
        register_market_exported_methods_v10_onwards!($registry, $code_cid, $state_version);

        use $state_version::{ActivateDealsParams, ComputeDataCommitmentParams, Method};

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::ActivateDeals, ActivateDealsParams),
                (Method::ComputeDataCommitment, ComputeDataCommitmentParams)
            ]
        );
    }};
}

macro_rules! register_market_versions_12 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        register_market_basic_methods!($registry, $code_cid, $state_version);
        register_market_exported_methods_v10_onwards!($registry, $code_cid, $state_version);

        use $state_version::{BatchActivateDealsParams, Method};

        register_actor_methods!(
            $registry,
            $code_cid,
            [(Method::BatchActivateDeals, BatchActivateDealsParams)]
        );
    }};
}

macro_rules! register_market_versions_onwards {
    ($registry:expr, $code_cid:expr, $market_state_version:path) => {{
        register_market_basic_methods!($registry, $code_cid, $market_state_version);
        register_market_exported_methods_v10_onwards!($registry, $code_cid, $market_state_version);
        register_market_exported_methods_v13_onwards!($registry, $code_cid, $market_state_version);

        use $market_state_version::{BatchActivateDealsParams, Method};

        register_actor_methods!(
            $registry,
            $code_cid,
            [(Method::BatchActivateDeals, BatchActivateDealsParams),]
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
            register_market_versions_8_to_9!(registry, cid, fil_actor_market_state::v8)
        }
        ActorVersion::V9 => {
            register_market_versions_8_to_9!(registry, cid, fil_actor_market_state::v9)
        }
        ActorVersion::V10 => {
            register_market_versions_10_to_11!(registry, cid, fil_actor_market_state::v10)
        }
        ActorVersion::V11 => {
            register_market_versions_10_to_11!(registry, cid, fil_actor_market_state::v11)
        }
        ActorVersion::V12 => {
            register_market_versions_12!(registry, cid, fil_actor_market_state::v12)
        }
        ActorVersion::V13 => {
            register_market_versions_onwards!(registry, cid, fil_actor_market_state::v13)
        }
        ActorVersion::V14 => {
            register_market_versions_onwards!(registry, cid, fil_actor_market_state::v14)
        }
        ActorVersion::V15 => {
            register_market_versions_onwards!(registry, cid, fil_actor_market_state::v15)
        }
        ActorVersion::V16 => {
            register_market_versions_onwards!(registry, cid, fil_actor_market_state::v16)
        }
        ActorVersion::V17 => {
            register_market_versions_onwards!(registry, cid, fil_actor_market_state::v17)
        }
    }
}
