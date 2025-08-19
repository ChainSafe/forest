// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::address::Address;
use crate::shim::message::MethodNum;
use cid::Cid;

macro_rules! register_miner_basic_methods {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            ApplyRewardParams, ChangeMultiaddrsParams, ChangePeerIDParams,
            ChangeWorkerAddressParams, CheckSectorProvenParams, CompactPartitionsParams,
            CompactSectorNumbersParams, DeclareFaultsParams, DeclareFaultsRecoveredParams,
            DeferredCronEventParams, DisputeWindowedPoStParams, ExtendSectorExpirationParams,
            Method, MinerConstructorParams, ProveCommitAggregateParams, ProveReplicaUpdatesParams,
            ReportConsensusFaultParams, SubmitWindowedPoStParams, TerminateSectorsParams,
            WithdrawBalanceParams,
        };

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, MinerConstructorParams),
                (Method::ChangeWorkerAddress, ChangeWorkerAddressParams),
                (Method::ChangePeerID, ChangePeerIDParams),
                (Method::SubmitWindowedPoSt, SubmitWindowedPoStParams),
                (Method::ExtendSectorExpiration, ExtendSectorExpirationParams),
                (Method::TerminateSectors, TerminateSectorsParams),
                (Method::DeclareFaults, DeclareFaultsParams),
                (Method::DeclareFaultsRecovered, DeclareFaultsRecoveredParams),
                (Method::OnDeferredCronEvent, DeferredCronEventParams),
                (Method::CheckSectorProven, CheckSectorProvenParams),
                (Method::ApplyRewards, ApplyRewardParams),
                (Method::ReportConsensusFault, ReportConsensusFaultParams),
                (Method::WithdrawBalance, WithdrawBalanceParams),
                (Method::ChangeMultiaddrs, ChangeMultiaddrsParams),
                (Method::CompactPartitions, CompactPartitionsParams),
                (Method::CompactSectorNumbers, CompactSectorNumbersParams),
                (Method::DisputeWindowedPoSt, DisputeWindowedPoStParams),
                (Method::ProveCommitAggregate, ProveCommitAggregateParams),
                (Method::ProveReplicaUpdates, ProveReplicaUpdatesParams),
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::ControlAddresses, empty),
                (Method::RepayDebt, empty),
            ]
        );
    }};
}

macro_rules! register_miner_common_methods_v10_onwards {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        register_miner_basic_methods!($registry, $code_cid, $state_version);

        use $state_version::{
            ChangeBeneficiaryParams, ChangeMultiaddrsParams, ChangePeerIDParams,
            ChangeWorkerAddressParams, ExtendSectorExpiration2Params, IsControllingAddressParam,
            Method, PreCommitSectorBatchParams2, WithdrawBalanceParams,
        };

        // Register methods with parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::PreCommitSectorBatch2, PreCommitSectorBatchParams2),
                (Method::ChangeBeneficiary, ChangeBeneficiaryParams),
                (
                    Method::ExtendSectorExpiration2,
                    ExtendSectorExpiration2Params
                ),
                (Method::ChangePeerIDExported, ChangePeerIDParams),
                (Method::WithdrawBalanceExported, WithdrawBalanceParams),
                (Method::ChangeMultiaddrsExported, ChangeMultiaddrsParams),
                (Method::ChangeBeneficiaryExported, ChangeBeneficiaryParams),
                (
                    Method::IsControllingAddressExported,
                    IsControllingAddressParam
                ),
                (
                    Method::ChangeWorkerAddressExported,
                    ChangeWorkerAddressParams
                ),
            ]
        );

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::GetBeneficiary, empty),
                (Method::ConfirmChangeWorkerAddress, empty),
                (Method::ConfirmChangeWorkerAddressExported, empty),
                (Method::RepayDebtExported, empty),
                (Method::GetBeneficiaryExported, empty),
                (Method::GetOwnerExported, empty),
                (Method::GetSectorSizeExported, empty),
                (Method::GetAvailableBalanceExported, empty),
                (Method::GetVestingFundsExported, empty),
                (Method::GetPeerIDExported, empty),
                (Method::GetMultiaddrsExported, empty),
            ]
        );
    }};
}

macro_rules! register_miner_common_method_v14_onwards {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            ChangeOwnerAddressParams, Method, ProveCommitSectors3Params,
            ProveCommitSectorsNIParams, ProveReplicaUpdates3Params,
        };
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::ProveCommitSectors3, ProveCommitSectors3Params),
                (Method::ProveReplicaUpdates3, ProveReplicaUpdates3Params),
                (Method::ProveCommitSectorsNI, ProveCommitSectorsNIParams),
                (Method::ChangeOwnerAddress, ChangeOwnerAddressParams),
                (Method::ChangeOwnerAddressExported, ChangeOwnerAddressParams),
            ]
        );
    }};
}

fn register_miner_version_8(registry: &mut MethodRegistry, cid: Cid) {
    register_miner_basic_methods!(registry, cid, fil_actor_miner_state::v8);

    use fil_actor_miner_state::v8::{
        ConfirmSectorProofsParams, Method, PreCommitSectorBatchParams, PreCommitSectorParams,
        ProveCommitSectorParams,
    };

    register_actor_methods!(
        registry,
        cid,
        [
            (Method::ChangeOwnerAddress, Address),
            (Method::PreCommitSector, PreCommitSectorParams),
            (Method::ProveCommitSector, ProveCommitSectorParams),
            (Method::PreCommitSectorBatch, PreCommitSectorBatchParams),
            (Method::ConfirmSectorProofsValid, ConfirmSectorProofsParams),
        ]
    );

    register_actor_methods!(registry, cid, [(Method::ConfirmUpdateWorkerKey, empty)]);
}

fn register_miner_version_9(registry: &mut MethodRegistry, cid: Cid) {
    register_miner_basic_methods!(registry, cid, fil_actor_miner_state::v9);

    use fil_actor_miner_state::v9::{
        ChangeBeneficiaryParams, ConfirmSectorProofsParams, ExtendSectorExpiration2Params, Method,
        PreCommitSectorBatchParams, PreCommitSectorBatchParams2, PreCommitSectorParams,
        ProveCommitSectorParams, ProveReplicaUpdatesParams2,
    };

    register_actor_methods!(
        registry,
        cid,
        [
            (Method::PreCommitSector, PreCommitSectorParams),
            (Method::ProveCommitSector, ProveCommitSectorParams),
            (Method::PreCommitSectorBatch, PreCommitSectorBatchParams),
            (Method::PreCommitSectorBatch2, PreCommitSectorBatchParams2),
            (Method::ChangeOwnerAddress, Address),
            (Method::ProveReplicaUpdates2, ProveReplicaUpdatesParams2),
            (Method::ChangeBeneficiary, ChangeBeneficiaryParams),
            (
                Method::ExtendSectorExpiration2,
                ExtendSectorExpiration2Params
            ),
            (Method::ConfirmSectorProofsValid, ConfirmSectorProofsParams),
        ]
    );

    register_actor_methods!(
        registry,
        cid,
        [
            (Method::GetBeneficiary, empty),
            (Method::ConfirmUpdateWorkerKey, empty)
        ]
    );
}

fn register_miner_version_10(registry: &mut MethodRegistry, cid: Cid) {
    register_miner_common_methods_v10_onwards!(registry, cid, fil_actor_miner_state::v10);

    use fil_actor_miner_state::v10::{
        ConfirmSectorProofsParams, ExtendSectorExpiration2Params, Method,
        PreCommitSectorBatchParams, PreCommitSectorParams, ProveCommitSectorParams,
        ProveReplicaUpdatesParams2,
    };

    register_actor_methods!(
        registry,
        cid,
        [
            (Method::PreCommitSector, PreCommitSectorParams),
            (Method::ProveReplicaUpdates2, ProveReplicaUpdatesParams2),
            (Method::ProveCommitSector, ProveCommitSectorParams),
            (Method::PreCommitSectorBatch, PreCommitSectorBatchParams),
            (
                Method::ExtendSectorExpiration2,
                ExtendSectorExpiration2Params
            ),
            (Method::ChangeOwnerAddress, Address),
            (Method::ChangeOwnerAddressExported, Address),
            (Method::ConfirmSectorProofsValid, ConfirmSectorProofsParams),
        ]
    );
}

fn register_miner_version_11(registry: &mut MethodRegistry, cid: Cid) {
    register_miner_common_methods_v10_onwards!(registry, cid, fil_actor_miner_state::v11);

    use fil_actor_miner_state::v11::{
        ChangeOwnerAddressParams, ConfirmSectorProofsParams, ExtendSectorExpiration2Params, Method,
        PreCommitSectorBatchParams, PreCommitSectorParams, ProveCommitSectorParams,
        ProveReplicaUpdatesParams2,
    };

    register_actor_methods!(
        registry,
        cid,
        [
            (Method::PreCommitSector, PreCommitSectorParams),
            (Method::ProveReplicaUpdates2, ProveReplicaUpdatesParams2),
            (Method::ProveCommitSector, ProveCommitSectorParams),
            (Method::PreCommitSectorBatch, PreCommitSectorBatchParams),
            (
                Method::ExtendSectorExpiration2,
                ExtendSectorExpiration2Params
            ),
            (Method::ChangeOwnerAddress, ChangeOwnerAddressParams),
            (Method::ChangeOwnerAddressExported, ChangeOwnerAddressParams),
            (Method::ConfirmSectorProofsValid, ConfirmSectorProofsParams),
        ]
    );
}

fn register_miner_version_12(registry: &mut MethodRegistry, cid: Cid) {
    register_miner_common_methods_v10_onwards!(registry, cid, fil_actor_miner_state::v12);

    use fil_actor_miner_state::v12::{
        ChangeOwnerAddressParams, ConfirmSectorProofsParams, Method, PreCommitSectorBatchParams,
        PreCommitSectorParams, ProveCommitSectorParams, ProveReplicaUpdatesParams2,
    };
    register_actor_methods!(
        registry,
        cid,
        [
            (Method::PreCommitSector, PreCommitSectorParams),
            (Method::ProveReplicaUpdates2, ProveReplicaUpdatesParams2),
            (Method::ProveCommitSector, ProveCommitSectorParams),
            (Method::PreCommitSectorBatch, PreCommitSectorBatchParams),
            (Method::ChangeOwnerAddress, ChangeOwnerAddressParams),
            (Method::ChangeOwnerAddressExported, ChangeOwnerAddressParams),
            (Method::ConfirmSectorProofsValid, ConfirmSectorProofsParams),
        ]
    );
}

fn register_miner_version_13(registry: &mut MethodRegistry, cid: Cid) {
    register_miner_common_methods_v10_onwards!(registry, cid, fil_actor_miner_state::v13);

    use fil_actor_miner_state::v13::{
        ChangeOwnerAddressParams, ConfirmSectorProofsParams, Method, ProveCommitSectorParams,
        ProveCommitSectors3Params,
    };
    register_actor_methods!(
        registry,
        cid,
        [
            (Method::ProveCommitSector, ProveCommitSectorParams),
            (Method::ProveCommitSectors3, ProveCommitSectors3Params),
            (Method::ChangeOwnerAddress, ChangeOwnerAddressParams),
            (Method::ChangeOwnerAddressExported, ChangeOwnerAddressParams),
            (Method::ConfirmSectorProofsValid, ConfirmSectorProofsParams),
        ]
    );
}

fn register_miner_versions_14(registry: &mut MethodRegistry, cid: Cid) {
    register_miner_common_methods_v10_onwards!(registry, cid, fil_actor_miner_state::v14);
    register_miner_common_method_v14_onwards!(registry, cid, fil_actor_miner_state::v14);
}

fn register_miner_version_15(registry: &mut MethodRegistry, cid: Cid) {
    register_miner_common_methods_v10_onwards!(registry, cid, fil_actor_miner_state::v15);
    register_miner_common_method_v14_onwards!(registry, cid, fil_actor_miner_state::v15);
    use fil_actor_miner_state::v15::{InternalSectorSetupForPresealParams, Method};
    register_actor_methods!(
        registry,
        cid,
        [(
            Method::InternalSectorSetupForPreseal,
            InternalSectorSetupForPresealParams
        )]
    );
}

fn register_miner_version_16(registry: &mut MethodRegistry, cid: Cid) {
    register_miner_common_methods_v10_onwards!(registry, cid, fil_actor_miner_state::v16);
    register_miner_common_method_v14_onwards!(registry, cid, fil_actor_miner_state::v16);

    use fil_actor_miner_state::v16::{
        InternalSectorSetupForPresealParams, MaxTerminationFeeParams, Method,
    };
    register_actor_methods!(
        registry,
        cid,
        [
            (
                Method::InternalSectorSetupForPreseal,
                InternalSectorSetupForPresealParams
            ),
            (Method::MaxTerminationFeeExported, MaxTerminationFeeParams),
        ]
    );

    register_actor_methods!(registry, cid, [(Method::InitialPledgeExported, empty)]);
}

pub(crate) fn register_miner_actor_methods(registry: &mut MethodRegistry, cid: Cid, version: u64) {
    match version {
        8 => register_miner_version_8(registry, cid),
        9 => register_miner_version_9(registry, cid),
        10 => register_miner_version_10(registry, cid),
        11 => register_miner_version_11(registry, cid),
        12 => register_miner_version_12(registry, cid),
        13 => register_miner_version_13(registry, cid),
        14 => register_miner_versions_14(registry, cid),
        15 => register_miner_version_15(registry, cid),
        16 => register_miner_version_16(registry, cid),
        _ => {}
    }
}
