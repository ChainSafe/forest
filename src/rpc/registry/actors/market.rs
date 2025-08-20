// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use anyhow::Result;
use cid::Cid;

macro_rules! register_market_versions_8_to_9 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            AddBalanceParams, Method, PublishStorageDealsParams, VerifyDealsForActivationParams,
            WithdrawBalanceParams,
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
            AddBalanceParams, Method, PublishStorageDealsParams, VerifyDealsForActivationParams,
            WithdrawBalanceParams,
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
            AddBalanceParams, Method, PublishStorageDealsParams, VerifyDealsForActivationParams,
            WithdrawBalanceParams,
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

// var Methods = map[abi.MethodNum]builtin.MethodMeta{
// 	1: builtin.NewMethodMeta("Constructor", *new(func(*abi.EmptyValue) *abi.EmptyValue)), // Constructor
// 	2: builtin.NewMethodMeta("AddBalance", *new(func(*address.Address) *abi.EmptyValue)), // AddBalance
// 	builtin.MustGenerateFRCMethodNum("AddBalance"): builtin.NewMethodMeta("AddBalanceExported", *new(func(*address.Address) *abi.EmptyValue)), // AddBalanceExported
// 	3: builtin.NewMethodMeta("WithdrawBalance", *new(func(*WithdrawBalanceParams) *abi.TokenAmount)), // WithdrawBalance
// 	builtin.MustGenerateFRCMethodNum("WithdrawBalance"): builtin.NewMethodMeta("WithdrawBalanceExported", *new(func(*WithdrawBalanceParams) *abi.TokenAmount)), // WithdrawBalanceExported
// 	4: builtin.NewMethodMeta("PublishStorageDeals", *new(func(*PublishStorageDealsParams) *PublishStorageDealsReturn)), // PublishStorageDeals
// 	builtin.MustGenerateFRCMethodNum("PublishStorageDeals"): builtin.NewMethodMeta("PublishStorageDealsExported", *new(func(*PublishStorageDealsParams) *PublishStorageDealsReturn)), // PublishStorageDealsExported
// 	5: builtin.NewMethodMeta("VerifyDealsForActivation", *new(func(*VerifyDealsForActivationParams) *VerifyDealsForActivationReturn)), // VerifyDealsForActivation
// 	6: builtin.NewMethodMeta("ActivateDeals", *new(func(*ActivateDealsParams) *abi.EmptyValue)),                                       // ActivateDeals
// 	7: builtin.NewMethodMeta("OnMinerSectorsTerminate", *new(func(*OnMinerSectorsTerminateParams) *abi.EmptyValue)),                   // OnMinerSectorsTerminate
// 	8: builtin.NewMethodMeta("ComputeDataCommitment", nil),                                                                            // deprecated
// 	9: builtin.NewMethodMeta("CronTick", *new(func(*abi.EmptyValue) *abi.EmptyValue)),                                                 // CronTick
// 	builtin.MustGenerateFRCMethodNum("GetBalance"):                builtin.NewMethodMeta("GetBalanceExported", *new(func(*address.Address) *GetBalanceReturn)),                                               // GetBalanceExported
// 	builtin.MustGenerateFRCMethodNum("GetDealDataCommitment"):     builtin.NewMethodMeta("GetDealDataCommitmentExported", *new(func(*GetDealDataCommitmentParams) *GetDealDataCommitmentReturn)),             // GetDealDataCommitmentExported
// 	builtin.MustGenerateFRCMethodNum("GetDealClient"):             builtin.NewMethodMeta("GetDealClientExported", *new(func(*GetDealClientParams) *GetDealClientReturn)),                                     // GetDealClientExported
// 	builtin.MustGenerateFRCMethodNum("GetDealProvider"):           builtin.NewMethodMeta("GetDealProviderExported", *new(func(*GetDealProviderParams) *GetDealProviderReturn)),                               // GetDealProviderExported
// 	builtin.MustGenerateFRCMethodNum("GetDealLabel"):              builtin.NewMethodMeta("GetDealLabelExported", *new(func(*GetDealLabelParams) *GetDealLabelReturn)),                                        // GetDealLabelExported
// 	builtin.MustGenerateFRCMethodNum("GetDealTerm"):               builtin.NewMethodMeta("GetDealTermExported", *new(func(*GetDealTermParams) *GetDealTermReturn)),                                           // GetDealTermExported
// 	builtin.MustGenerateFRCMethodNum("GetDealTotalPrice"):         builtin.NewMethodMeta("GetDealTotalPriceExported", *new(func(*GetDealTotalPriceParams) *GetDealTotalPriceReturn)),                         // GetDealTotalPriceExported
// 	builtin.MustGenerateFRCMethodNum("GetDealClientCollateral"):   builtin.NewMethodMeta("GetDealClientCollateralExported", *new(func(*GetDealClientCollateralParams) *GetDealClientCollateralReturn)),       // GetDealClientCollateralExported
// 	builtin.MustGenerateFRCMethodNum("GetDealProviderCollateral"): builtin.NewMethodMeta("GetDealProviderCollateralExported", *new(func(*GetDealProviderCollateralParams) *GetDealProviderCollateralReturn)), // GetDealProviderCollateralExported
// 	builtin.MustGenerateFRCMethodNum("GetDealVerified"):           builtin.NewMethodMeta("GetDealVerifiedExported", *new(func(*GetDealVerifiedParams) *GetDealVerifiedReturn)),                               // GetDealVerifiedExported
// 	builtin.MustGenerateFRCMethodNum("GetDealActivation"):         builtin.NewMethodMeta("GetDealActivationExported", *new(func(*GetDealActivationParams) *GetDealActivationReturn)),                         // GetDealActivationExported
// 	builtin.MustGenerateFRCMethodNum("GetDealSector"):             builtin.NewMethodMeta("GetDealSectorExported", *new(func(*GetDealSectorParams) *GetDealSectorReturn)),                                     // GetDealSectorExported
// 	builtin.MethodSectorContentChanged:                            builtin.NewMethodMeta("SectorContentChanged", *new(func(*miner.SectorContentChangedParams) *miner.SectorContentChangedReturn)),            // SectorContentChanged
// 	builtin.MustGenerateFRCMethodNum("SettleDealPayments"):        builtin.NewMethodMeta("SettleDealPaymentsExported", *new(func(*SettleDealPaymentsParams) *SettleDealPaymentsReturn)),                      // SettleDealPaymentsExported
// }
macro_rules! register_market_versions_13_to_16 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            AddBalanceParams, Method, PublishStorageDealsParams, VerifyDealsForActivationParams,
            WithdrawBalanceParams,
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
