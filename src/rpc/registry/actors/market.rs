// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::address::Address;
use crate::shim::message::MethodNum;
use cid::Cid;

macro_rules! register_market_versions_8_to_9 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            ActivateDealsParams, AddBalanceParams, ComputeDataCommitmentParams, Method,
            OnMinerSectorsTerminateParams, PublishStorageDealsParams,
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
                (Method::ActivateDeals, ActivateDealsParams),
                (
                    Method::OnMinerSectorsTerminate,
                    OnMinerSectorsTerminateParams
                ),
                (Method::ComputeDataCommitment, ComputeDataCommitmentParams)
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

macro_rules! register_market_versions_10_to_11 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            ActivateDealsParams, AddBalanceParams, ComputeDataCommitmentParams,
            GetDealActivationParams, GetDealClientCollateralParams, GetDealClientParams,
            GetDealDataCommitmentParams, GetDealLabelParams, GetDealProviderCollateralParams,
            GetDealProviderParams, GetDealTermParams, GetDealTotalPriceParams,
            GetDealVerifiedParams, Method, OnMinerSectorsTerminateParams,
            PublishStorageDealsParams, VerifyDealsForActivationParams, WithdrawBalanceParams,
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
                (Method::ActivateDeals, ActivateDealsParams),
                (
                    Method::OnMinerSectorsTerminate,
                    OnMinerSectorsTerminateParams
                ),
                (Method::ComputeDataCommitment, ComputeDataCommitmentParams)
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [(Method::Constructor, empty), (Method::CronTick, empty),]
        );

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

macro_rules! register_market_versions_12 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            AddBalanceParams, BatchActivateDealsParams, GetDealActivationParams,
            GetDealClientCollateralParams, GetDealClientParams, GetDealDataCommitmentParams,
            GetDealLabelParams, GetDealProviderCollateralParams, GetDealProviderParams,
            GetDealTermParams, GetDealTotalPriceParams, GetDealVerifiedParams, Method,
            OnMinerSectorsTerminateParams, PublishStorageDealsParams,
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
                (Method::BatchActivateDeals, BatchActivateDealsParams),
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

macro_rules! register_market_versions_13_to_16 {
    ($registry:expr, $code_cid:expr, $market_state_version:path, $miner_state_version:path) => {{
        use $market_state_version::{
            AddBalanceParams, BatchActivateDealsParams, GetDealActivationParams,
            GetDealClientCollateralParams, GetDealClientParams, GetDealDataCommitmentParams,
            GetDealLabelParams, GetDealProviderCollateralParams, GetDealProviderParams,
            GetDealSectorParams, GetDealTermParams, GetDealTotalPriceParams, GetDealVerifiedParams,
            Method, OnMinerSectorsTerminateParams, PublishStorageDealsParams,
            SettleDealPaymentsParams, VerifyDealsForActivationParams, WithdrawBalanceParams,
        };
        // When using a macro variable for a module path in a `use` statement,
        // Rust can get confused if we write `use $miner_state_version::SectorContentChangedParams;` directly.
        // Wrapping the imported item in `{ ... }` disambiguates the path for the compiler and ensures
        // the import works correctly even when `$miner_state_version` expands to a module path.
        // Also, to avoid rustfmt automatically reformatting this line, we use #[rustfmt::skip].
        #[rustfmt::skip]
        use $miner_state_version::{SectorContentChangedParams};

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
                (Method::BatchActivateDeals, BatchActivateDealsParams),
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

pub(crate) fn register_actor_methods(registry: &mut MethodRegistry, cid: Cid, version: u64) {
    match version {
        8 => register_market_versions_8_to_9!(registry, cid, fil_actor_market_state::v8),
        9 => register_market_versions_8_to_9!(registry, cid, fil_actor_market_state::v9),
        10 => register_market_versions_10_to_11!(registry, cid, fil_actor_market_state::v10),
        11 => register_market_versions_10_to_11!(registry, cid, fil_actor_market_state::v11),
        12 => register_market_versions_12!(registry, cid, fil_actor_market_state::v12),
        13 => register_market_versions_13_to_16!(
            registry,
            cid,
            fil_actor_market_state::v13,
            fil_actor_miner_state::v13
        ),
        14 => register_market_versions_13_to_16!(
            registry,
            cid,
            fil_actor_market_state::v14,
            fil_actor_miner_state::v14
        ),
        15 => register_market_versions_13_to_16!(
            registry,
            cid,
            fil_actor_market_state::v15,
            fil_actor_miner_state::v15
        ),
        16 => register_market_versions_13_to_16!(
            registry,
            cid,
            fil_actor_market_state::v16,
            fil_actor_miner_state::v16
        ),
        _ => {}
    }
}
