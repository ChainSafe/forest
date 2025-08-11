// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use anyhow::Result;
use cid::Cid;

// Macro for versions 8-9 (basic methods only)
macro_rules! register_miner_versions_8_to_9 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            ChangeMultiaddrsParams, ChangePeerIDParams, ChangeWorkerAddressParams,
            DeclareFaultsParams, DeclareFaultsRecoveredParams, DisputeWindowedPoStParams, Method,
            MinerConstructorParams, SubmitWindowedPoStParams, TerminateSectorsParams,
            WithdrawBalanceParams, PreCommitSectorBatchParams,
        };

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, MinerConstructorParams),
                (Method::ChangeWorkerAddress, ChangeWorkerAddressParams),
                (Method::ChangePeerID, ChangePeerIDParams),
                (Method::SubmitWindowedPoSt, SubmitWindowedPoStParams),
                (Method::TerminateSectors, TerminateSectorsParams),
                (Method::DeclareFaults, DeclareFaultsParams),
                (Method::DeclareFaultsRecovered, DeclareFaultsRecoveredParams),
                (Method::WithdrawBalance, WithdrawBalanceParams),
                (Method::ChangeMultiaddrs, ChangeMultiaddrsParams),
                (Method::DisputeWindowedPoSt, DisputeWindowedPoStParams),
                (Method::PreCommitSectorBatch, PreCommitSectorBatchParams),
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::ProveCommitAggregate, empty),
                (Method::ProveReplicaUpdates, empty),
                (Method::ControlAddresses, empty),
                (Method::OnDeferredCronEvent, empty),
                (Method::CheckSectorProven, empty),
                (Method::ApplyRewards, empty),
                (Method::RepayDebt, empty),
            ]
        );
    }};
}

// Macro for versions 10-11 (add ChangeBeneficiary, ExtendSectorExpiration2, Compact methods)
macro_rules! register_miner_versions_10_to_11 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            ChangeBeneficiaryParams, ChangeMultiaddrsParams, ChangePeerIDParams,
            ChangeWorkerAddressParams, CompactPartitionsParams, CompactSectorNumbersParams,
            DeclareFaultsParams, DeclareFaultsRecoveredParams, DisputeWindowedPoStParams,
            ExtendSectorExpiration2Params, Method, MinerConstructorParams,
            SubmitWindowedPoStParams, TerminateSectorsParams, WithdrawBalanceParams,
        };

        // Register methods with parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, MinerConstructorParams),
                (Method::ChangeWorkerAddress, ChangeWorkerAddressParams),
                (Method::ChangePeerID, ChangePeerIDParams),
                (Method::SubmitWindowedPoSt, SubmitWindowedPoStParams),
                (Method::TerminateSectors, TerminateSectorsParams),
                (Method::DeclareFaults, DeclareFaultsParams),
                (Method::DeclareFaultsRecovered, DeclareFaultsRecoveredParams),
                (Method::WithdrawBalance, WithdrawBalanceParams),
                (Method::ChangeMultiaddrs, ChangeMultiaddrsParams),
                (Method::DisputeWindowedPoSt, DisputeWindowedPoStParams),
                (Method::ChangeBeneficiary, ChangeBeneficiaryParams),
                (
                    Method::ExtendSectorExpiration2,
                    ExtendSectorExpiration2Params
                ),
                (Method::CompactPartitions, CompactPartitionsParams),
                (Method::CompactSectorNumbers, CompactSectorNumbersParams),
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::ProveCommitAggregate, empty),
                (Method::ProveReplicaUpdates, empty),
                (Method::ControlAddresses, empty),
                (Method::OnDeferredCronEvent, empty),
                (Method::CheckSectorProven, empty),
                (Method::ApplyRewards, empty),
                (Method::RepayDebt, empty),
                (Method::ConfirmChangeWorkerAddress, empty),
                (Method::GetBeneficiary, empty),
                (Method::GetOwnerExported, empty),
                (Method::IsControllingAddressExported, empty),
                (Method::GetSectorSizeExported, empty),
                (Method::GetAvailableBalanceExported, empty),
                (Method::GetVestingFundsExported, empty),
                (Method::GetPeerIDExported, empty),
                (Method::GetMultiaddrsExported, empty),
            ]
        );
    }};
}

// Macro for versions 12 (same as v10-11, but no ProveCommitSectors3/ProveReplicaUpdates3)
macro_rules! register_miner_version_12 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            ChangeBeneficiaryParams, ChangeMultiaddrsParams, ChangePeerIDParams,
            ChangeWorkerAddressParams, CompactPartitionsParams, CompactSectorNumbersParams,
            DeclareFaultsParams, DeclareFaultsRecoveredParams, DisputeWindowedPoStParams,
            ExtendSectorExpiration2Params, Method, MinerConstructorParams,
            SubmitWindowedPoStParams, TerminateSectorsParams, WithdrawBalanceParams,
        };

        // Register methods with parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, MinerConstructorParams),
                (Method::ChangeWorkerAddress, ChangeWorkerAddressParams),
                (Method::ChangePeerID, ChangePeerIDParams),
                (Method::SubmitWindowedPoSt, SubmitWindowedPoStParams),
                (Method::TerminateSectors, TerminateSectorsParams),
                (Method::DeclareFaults, DeclareFaultsParams),
                (Method::DeclareFaultsRecovered, DeclareFaultsRecoveredParams),
                (Method::WithdrawBalance, WithdrawBalanceParams),
                (Method::ChangeMultiaddrs, ChangeMultiaddrsParams),
                (Method::DisputeWindowedPoSt, DisputeWindowedPoStParams),
                (Method::ChangeBeneficiary, ChangeBeneficiaryParams),
                (
                    Method::ExtendSectorExpiration2,
                    ExtendSectorExpiration2Params
                ),
                (Method::CompactPartitions, CompactPartitionsParams),
                (Method::CompactSectorNumbers, CompactSectorNumbersParams),
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::ProveCommitAggregate, empty),
                (Method::ProveReplicaUpdates, empty),
                (Method::ControlAddresses, empty),
                (Method::OnDeferredCronEvent, empty),
                (Method::CheckSectorProven, empty),
                (Method::ApplyRewards, empty),
                (Method::RepayDebt, empty),
                (Method::ConfirmChangeWorkerAddress, empty),
                (Method::GetBeneficiary, empty),
                (Method::GetOwnerExported, empty),
                (Method::IsControllingAddressExported, empty),
                (Method::GetSectorSizeExported, empty),
                (Method::GetAvailableBalanceExported, empty),
                (Method::GetVestingFundsExported, empty),
                (Method::GetPeerIDExported, empty),
                (Method::GetMultiaddrsExported, empty),
            ]
        );
    }};
}

// Macro for versions 13-15 (add ProveCommitSectors3, ProveReplicaUpdates3)
macro_rules! register_miner_versions_13_to_15 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            ChangeBeneficiaryParams, ChangeMultiaddrsParams, ChangePeerIDParams,
            ChangeWorkerAddressParams, CompactPartitionsParams, CompactSectorNumbersParams,
            DeclareFaultsParams, DeclareFaultsRecoveredParams, DisputeWindowedPoStParams,
            ExtendSectorExpiration2Params, Method, MinerConstructorParams,
            ProveCommitSectors3Params, ProveReplicaUpdates3Params, SubmitWindowedPoStParams,
            TerminateSectorsParams, WithdrawBalanceParams,
        };

        // Register methods with parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, MinerConstructorParams),
                (Method::ChangeWorkerAddress, ChangeWorkerAddressParams),
                (Method::ChangePeerID, ChangePeerIDParams),
                (Method::SubmitWindowedPoSt, SubmitWindowedPoStParams),
                (Method::TerminateSectors, TerminateSectorsParams),
                (Method::DeclareFaults, DeclareFaultsParams),
                (Method::DeclareFaultsRecovered, DeclareFaultsRecoveredParams),
                (Method::WithdrawBalance, WithdrawBalanceParams),
                (Method::ChangeMultiaddrs, ChangeMultiaddrsParams),
                (Method::DisputeWindowedPoSt, DisputeWindowedPoStParams),
                (Method::ChangeBeneficiary, ChangeBeneficiaryParams),
                (
                    Method::ExtendSectorExpiration2,
                    ExtendSectorExpiration2Params
                ),
                (Method::CompactPartitions, CompactPartitionsParams),
                (Method::CompactSectorNumbers, CompactSectorNumbersParams),
                (Method::ProveCommitSectors3, ProveCommitSectors3Params),
                (Method::ProveReplicaUpdates3, ProveReplicaUpdates3Params),
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::ProveCommitAggregate, empty),
                (Method::ProveReplicaUpdates, empty),
                (Method::ControlAddresses, empty),
                (Method::OnDeferredCronEvent, empty),
                (Method::CheckSectorProven, empty),
                (Method::ApplyRewards, empty),
                (Method::RepayDebt, empty),
                (Method::ConfirmChangeWorkerAddress, empty),
                (Method::GetBeneficiary, empty),
                (Method::GetOwnerExported, empty),
                (Method::IsControllingAddressExported, empty),
                (Method::GetSectorSizeExported, empty),
                (Method::GetAvailableBalanceExported, empty),
                (Method::GetVestingFundsExported, empty),
                (Method::GetPeerIDExported, empty),
                (Method::GetMultiaddrsExported, empty),
            ]
        );
    }};
}

// Macro for version 16 (add ChangeOwnerAddress)
macro_rules! register_miner_version_16 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            ChangeBeneficiaryParams, ChangeMultiaddrsParams, ChangeOwnerAddressParams,
            ChangePeerIDParams, ChangeWorkerAddressParams, CompactPartitionsParams,
            CompactSectorNumbersParams, DeclareFaultsParams, DeclareFaultsRecoveredParams,
            DisputeWindowedPoStParams, ExtendSectorExpiration2Params, Method,
            MinerConstructorParams, ProveCommitSectors3Params, ProveReplicaUpdates3Params,
            SubmitWindowedPoStParams, TerminateSectorsParams, WithdrawBalanceParams,
        };

        // Register methods with parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, MinerConstructorParams),
                (Method::ChangeWorkerAddress, ChangeWorkerAddressParams),
                (Method::ChangePeerID, ChangePeerIDParams),
                (Method::SubmitWindowedPoSt, SubmitWindowedPoStParams),
                (Method::TerminateSectors, TerminateSectorsParams),
                (Method::DeclareFaults, DeclareFaultsParams),
                (Method::DeclareFaultsRecovered, DeclareFaultsRecoveredParams),
                (Method::WithdrawBalance, WithdrawBalanceParams),
                (Method::ChangeMultiaddrs, ChangeMultiaddrsParams),
                (Method::DisputeWindowedPoSt, DisputeWindowedPoStParams),
                (Method::ChangeBeneficiary, ChangeBeneficiaryParams),
                (
                    Method::ExtendSectorExpiration2,
                    ExtendSectorExpiration2Params
                ),
                (Method::CompactPartitions, CompactPartitionsParams),
                (Method::CompactSectorNumbers, CompactSectorNumbersParams),
                (Method::ProveCommitSectors3, ProveCommitSectors3Params),
                (Method::ProveReplicaUpdates3, ProveReplicaUpdates3Params),
                (Method::ChangeOwnerAddress, ChangeOwnerAddressParams),
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::ProveCommitAggregate, empty),
                (Method::ProveReplicaUpdates, empty),
                (Method::ControlAddresses, empty),
                (Method::OnDeferredCronEvent, empty),
                (Method::CheckSectorProven, empty),
                (Method::ApplyRewards, empty),
                (Method::RepayDebt, empty),
                (Method::ConfirmChangeWorkerAddress, empty),
                (Method::GetBeneficiary, empty),
                (Method::GetOwnerExported, empty),
                (Method::IsControllingAddressExported, empty),
                (Method::GetSectorSizeExported, empty),
                (Method::GetAvailableBalanceExported, empty),
                (Method::GetVestingFundsExported, empty),
                (Method::GetPeerIDExported, empty),
                (Method::GetMultiaddrsExported, empty),
            ]
        );
    }};
}

pub(crate) fn register_miner_actor_methods(registry: &mut MethodRegistry, cid: Cid, version: u64) {
    match version {
        8 => register_miner_versions_8_to_9!(registry, cid, fil_actor_miner_state::v8),
        9 => register_miner_versions_8_to_9!(registry, cid, fil_actor_miner_state::v9),
        10 => register_miner_versions_10_to_11!(registry, cid, fil_actor_miner_state::v10),
        11 => register_miner_versions_10_to_11!(registry, cid, fil_actor_miner_state::v11),
        12 => register_miner_version_12!(registry, cid, fil_actor_miner_state::v12),
        13 => register_miner_versions_13_to_15!(registry, cid, fil_actor_miner_state::v13),
        14 => register_miner_versions_13_to_15!(registry, cid, fil_actor_miner_state::v14),
        15 => register_miner_versions_13_to_15!(registry, cid, fil_actor_miner_state::v15),
        16 => register_miner_version_16!(registry, cid, fil_actor_miner_state::v16),
        _ => {}
    }
}
