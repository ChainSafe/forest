// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::address::Address;
use crate::shim::message::MethodNum;
use anyhow::Result;
use cid::Cid;

fn register_miner_version_8(registry: &mut MethodRegistry, cid: Cid) {
    use fil_actor_miner_state::v8::{
        ApplyRewardParams, ChangeMultiaddrsParams, ChangePeerIDParams, ChangeWorkerAddressParams,
        CheckSectorProvenParams, CompactPartitionsParams, CompactSectorNumbersParams,
        ConfirmSectorProofsParams, DeclareFaultsParams, DeclareFaultsRecoveredParams,
        DeferredCronEventParams, DisputeWindowedPoStParams, ExtendSectorExpirationParams, Method,
        MinerConstructorParams, PreCommitSectorBatchParams, PreCommitSectorParams,
        ProveCommitAggregateParams, ProveCommitSectorParams, ProveReplicaUpdatesParams,
        ReportConsensusFaultParams, SubmitWindowedPoStParams, TerminateSectorsParams,
        WithdrawBalanceParams,
    };

    register_actor_methods!(
        registry,
        cid,
        [
            (Method::Constructor, MinerConstructorParams),
            (Method::ChangeWorkerAddress, ChangeWorkerAddressParams),
            (Method::ChangePeerID, ChangePeerIDParams),
            (Method::SubmitWindowedPoSt, SubmitWindowedPoStParams),
            (Method::PreCommitSector, PreCommitSectorParams),
            (Method::ProveCommitSector, ProveCommitSectorParams),
            (Method::ExtendSectorExpiration, ExtendSectorExpirationParams),
            (Method::TerminateSectors, TerminateSectorsParams),
            (Method::DeclareFaults, DeclareFaultsParams),
            (Method::DeclareFaultsRecovered, DeclareFaultsRecoveredParams),
            (Method::OnDeferredCronEvent, DeferredCronEventParams),
            (Method::CheckSectorProven, CheckSectorProvenParams),
            (Method::ApplyRewards, ApplyRewardParams),
            (Method::ReportConsensusFault, ReportConsensusFaultParams),
            (Method::WithdrawBalance, WithdrawBalanceParams),
            (Method::ConfirmSectorProofsValid, ConfirmSectorProofsParams),
            (Method::ChangeMultiaddrs, ChangeMultiaddrsParams),
            (Method::CompactPartitions, CompactPartitionsParams),
            (Method::CompactSectorNumbers, CompactSectorNumbersParams),
            (Method::ChangeOwnerAddress, Address),
            (Method::DisputeWindowedPoSt, DisputeWindowedPoStParams),
            (Method::PreCommitSectorBatch, PreCommitSectorBatchParams),
            (Method::ProveCommitAggregate, ProveCommitAggregateParams),
            (Method::ProveReplicaUpdates, ProveReplicaUpdatesParams),
        ]
    );

    // Register methods without parameters
    register_actor_methods!(
        registry,
        cid,
        [
            (Method::ControlAddresses, empty),
            (Method::ConfirmUpdateWorkerKey, empty),
            (Method::RepayDebt, empty),
        ]
    );
}

fn register_miner_version_9(registry: &mut MethodRegistry, cid: Cid) {
    register_miner_version_8(registry, cid);

    use fil_actor_miner_state::v9::{
        ChangeBeneficiaryParams, ExtendSectorExpiration2Params, Method,
        PreCommitSectorBatchParams2, ProveReplicaUpdatesParams2,
    };

    register_actor_methods!(
        registry,
        cid,
        [
            (Method::PreCommitSectorBatch2, PreCommitSectorBatchParams2),
            (Method::ProveReplicaUpdates2, ProveReplicaUpdatesParams2),
            (Method::ChangeBeneficiary, ChangeBeneficiaryParams),
            (
                Method::ExtendSectorExpiration2,
                ExtendSectorExpiration2Params
            ),
        ]
    );

    register_actor_methods!(registry, cid, [(Method::GetBeneficiary, empty),]);
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
        8 => register_miner_version_8(registry, cid),
        9 => register_miner_version_9(registry, cid),
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
