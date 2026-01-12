// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use fil_actor_miner_state::v17::*;
use fil_actors_shared::v17::reward::FilterEstimate;
use fvm_ipld_encoding::{BytesDe, RawBytes};
use fvm_shared4::randomness::Randomness;

/// Creates state decode params tests for the Miner actor.
pub fn create_tests(tipset: &Tipset) -> Result<Vec<RpcTest>> {
    let miner_constructor_params = MinerConstructorParams {
        owner: Address::new_id(1000).into(),
        worker: Address::new_id(1001).into(),
        control_addresses: vec![Address::new_id(1002).into(), Address::new_id(1003).into()],
        window_post_proof_type: fvm_shared4::sector::RegisteredPoStProof::StackedDRGWindow32GiBV1P1,
        peer_id: b"miner".to_vec(),
        multi_addresses: Default::default(),
    };

    let miner_change_worker_params = ChangeWorkerAddressParams {
        new_worker: Address::new_id(2000).into(),
        new_control_addresses: vec![Address::new_id(2001).into()],
    };

    let miner_change_peer_id_params = ChangePeerIDParams {
        new_id: b"new_peer".to_vec(),
    };

    let miner_change_multiaddrs_params = ChangeMultiaddrsParams {
        new_multi_addrs: vec![BytesDe(vec![1, 2, 3])],
    };

    let miner_change_owner_params = ChangeOwnerAddressParams {
        new_owner: Address::new_id(3000).into(),
    };

    let miner_change_beneficiary_params = ChangeBeneficiaryParams {
        new_beneficiary: Address::new_id(4000).into(),
        new_quota: TokenAmount::from_atto(1000000000000000000u64).into(),
        new_expiration: 1000,
    };

    let miner_withdraw_balance_params = WithdrawBalanceParams {
        amount_requested: TokenAmount::from_atto(500000000000000000u64).into(),
    };

    let miner_submit_windowed_post_params = SubmitWindowedPoStParams {
        deadline: 0,
        partitions: vec![PoStPartition {
            index: 0,
            skipped: Default::default(),
        }],
        proofs: vec![],
        chain_commit_epoch: 0,
        chain_commit_rand: Randomness(vec![1, 22, 43]),
    };

    let miner_terminate_sectors_params = TerminateSectorsParams {
        terminations: vec![TerminationDeclaration {
            deadline: 0,
            partition: 0,
            sectors: Default::default(),
        }],
    };

    let miner_declare_faults_params = DeclareFaultsParams {
        faults: vec![FaultDeclaration {
            deadline: 0,
            partition: 0,
            sectors: Default::default(),
        }],
    };

    let miner_declare_faults_recovered_params = DeclareFaultsRecoveredParams {
        recoveries: vec![RecoveryDeclaration {
            deadline: 0,
            partition: 0,
            sectors: Default::default(),
        }],
    };

    let miner_deferred_cron_event_params = DeferredCronEventParams {
        event_payload: vec![],
        reward_smoothed: FilterEstimate {
            position: Default::default(),
            velocity: Default::default(),
        },
        quality_adj_power_smoothed: FilterEstimate {
            position: Default::default(),
            velocity: Default::default(),
        },
    };

    let miner_check_sector_proven_params = CheckSectorProvenParams { sector_number: 0 };

    let miner_apply_reward_params = ApplyRewardParams {
        reward: TokenAmount::from_atto(1000000000000000000u64).into(),
        penalty: TokenAmount::from_atto(0u64).into(),
    };

    let miner_report_consensus_fault_params = ReportConsensusFaultParams {
        header1: vec![],
        header2: vec![],
        header_extra: vec![],
    };

    let miner_compact_partitions_params = CompactPartitionsParams {
        deadline: 0,
        partitions: Default::default(),
    };

    let miner_compact_sector_numbers_params = CompactSectorNumbersParams {
        mask_sector_numbers: Default::default(),
    };

    let miner_dispute_windowed_post_params = DisputeWindowedPoStParams {
        deadline: 0,
        post_index: 0,
    };

    let miner_pre_commit_sector_batch2_params = PreCommitSectorBatchParams2 {
        sectors: vec![SectorPreCommitInfo {
            seal_proof: fvm_shared4::sector::RegisteredSealProof::StackedDRG2KiBV1P1,
            sector_number: 0,
            sealed_cid: Cid::default(),
            seal_rand_epoch: 0,
            deal_ids: vec![],
            expiration: 1000,
            unsealed_cid: CompactCommD(None),
        }],
    };

    let miner_extend_sector_expiration2_params = ExtendSectorExpiration2Params {
        extensions: vec![ExpirationExtension2 {
            deadline: 0,
            partition: 0,
            sectors: Default::default(),
            sectors_with_claims: vec![],
            new_expiration: 1000,
        }],
    };

    let miner_is_controlling_address_param = IsControllingAddressParam {
        address: Address::new_id(5000).into(),
    };

    let miner_prove_commit_sectors3_params = ProveCommitSectors3Params {
        sector_activations: vec![SectorActivationManifest {
            sector_number: 0,
            pieces: vec![PieceActivationManifest {
                cid: Cid::default(),
                size: fvm_shared4::piece::PaddedPieceSize(23),
                verified_allocation_key: None,
                notify: vec![],
            }],
        }],
        sector_proofs: vec![RawBytes::new(vec![])],
        aggregate_proof: RawBytes::new(vec![]),
        aggregate_proof_type: None,
        require_activation_success: true,
        require_notification_success: true,
    };

    let miner_prove_replica_updates3_params = ProveReplicaUpdates3Params {
        sector_updates: vec![SectorUpdateManifest {
            sector: 0,
            deadline: 0,
            partition: 0,
            new_sealed_cid: Cid::default(),
            pieces: vec![PieceActivationManifest {
                cid: Cid::default(),
                size: fvm_shared4::piece::PaddedPieceSize(12),
                verified_allocation_key: None,
                notify: vec![],
            }],
        }],
        sector_proofs: vec![RawBytes::new(vec![])],
        aggregate_proof: RawBytes::new(vec![]),
        update_proofs_type: fvm_shared4::sector::RegisteredUpdateProof::StackedDRG2KiBV1,
        aggregate_proof_type: None,
        require_activation_success: true,
        require_notification_success: true,
    };

    let miner_prove_commit_sectors_ni_params = ProveCommitSectorsNIParams {
        sectors: vec![SectorNIActivationInfo {
            sealing_number: 12,
            sealer_id: 23343,
            sealed_cid: Cid::default(),
            sector_number: 2343,
            seal_rand_epoch: 2343,
            expiration: 1000,
        }],
        aggregate_proof: RawBytes::new(vec![23, 2, 23]),
        seal_proof_type: fvm_shared4::sector::RegisteredSealProof::StackedDRG2KiBV1P1,
        aggregate_proof_type: fvm_shared4::sector::RegisteredAggregateProof::SnarkPackV1,
        proving_deadline: 234,
        require_activation_success: true,
    };

    let miner_internal_sector_setup_for_preseal_params = InternalSectorSetupForPresealParams {
        sectors: vec![0],
        reward_smoothed: FilterEstimate {
            position: Default::default(),
            velocity: Default::default(),
        },
        reward_baseline_power: Default::default(),
        quality_adj_power_smoothed: FilterEstimate {
            position: Default::default(),
            velocity: Default::default(),
        },
    };

    // let miner_max_termination_fee_params = MaxTerminationFeeParams {
    //     power: Default::default(),
    //     initial_pledge: TokenAmount::from_atto(1000000000000000000u64).into(),
    // };

    const MINER_ADDRESS: Address = Address::new_id(78216);
    Ok(vec![
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::Constructor as u64,
            to_vec(&miner_constructor_params)?,
            tipset.key().into(),
        ))?),
        // Methods without parameters
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ControlAddresses as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ChangeWorkerAddress as u64,
            to_vec(&miner_change_worker_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ChangePeerID as u64,
            to_vec(&miner_change_peer_id_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::SubmitWindowedPoSt as u64,
            to_vec(&miner_submit_windowed_post_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::TerminateSectors as u64,
            to_vec(&miner_terminate_sectors_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::DeclareFaults as u64,
            to_vec(&miner_declare_faults_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::DeclareFaultsRecovered as u64,
            to_vec(&miner_declare_faults_recovered_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::OnDeferredCronEvent as u64,
            to_vec(&miner_deferred_cron_event_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::CheckSectorProven as u64,
            to_vec(&miner_check_sector_proven_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ApplyRewards as u64,
            to_vec(&miner_apply_reward_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ReportConsensusFault as u64,
            to_vec(&miner_report_consensus_fault_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::WithdrawBalance as u64,
            to_vec(&miner_withdraw_balance_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::InternalSectorSetupForPreseal as u64,
            to_vec(&miner_internal_sector_setup_for_preseal_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ChangeMultiaddrs as u64,
            to_vec(&miner_change_multiaddrs_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::CompactPartitions as u64,
            to_vec(&miner_compact_partitions_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::CompactSectorNumbers as u64,
            to_vec(&miner_compact_sector_numbers_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ConfirmChangeWorkerAddress as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::RepayDebt as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ChangeOwnerAddress as u64,
            to_vec(&miner_change_owner_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::DisputeWindowedPoSt as u64,
            to_vec(&miner_dispute_windowed_post_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::PreCommitSectorBatch2 as u64,
            to_vec(&miner_pre_commit_sector_batch2_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ChangeBeneficiary as u64,
            to_vec(&miner_change_beneficiary_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::GetBeneficiary as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ExtendSectorExpiration2 as u64,
            to_vec(&miner_extend_sector_expiration2_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ProveCommitSectors3 as u64,
            to_vec(&miner_prove_commit_sectors3_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ProveReplicaUpdates3 as u64,
            to_vec(&miner_prove_replica_updates3_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ProveCommitSectorsNI as u64,
            to_vec(&miner_prove_commit_sectors_ni_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ChangeWorkerAddressExported as u64,
            to_vec(&miner_change_worker_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ChangePeerIDExported as u64,
            to_vec(&miner_change_peer_id_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::WithdrawBalanceExported as u64,
            to_vec(&miner_withdraw_balance_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ChangeMultiaddrsExported as u64,
            to_vec(&miner_change_multiaddrs_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ConfirmChangeWorkerAddressExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::RepayDebtExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ChangeOwnerAddressExported as u64,
            to_vec(&miner_change_owner_params)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::ChangeBeneficiaryExported as u64,
            to_vec(&miner_change_beneficiary_params)?,
            tipset.key().into(),
        ))?),
        // TODO(go-state-types): https://github.com/filecoin-project/go-state-types/issues/403
        // Enable this test once lotus starts supporting this.
        // RpcTest::identity(StateDecodeParams::request((
        //     MINER_ADDRESS,
        //     Method::GetBeneficiaryExported as u64,
        //     vec![],
        //     tipset.key().into(),
        // ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::GetOwnerExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::IsControllingAddressExported as u64,
            to_vec(&miner_is_controlling_address_param)?,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::GetSectorSizeExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::GetAvailableBalanceExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::GetVestingFundsExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::GetPeerIDExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDecodeParams::request((
            MINER_ADDRESS,
            Method::GetMultiaddrsExported as u64,
            vec![],
            tipset.key().into(),
        ))?),
        // TODO(go-state-types): https://github.com/filecoin-project/go-state-types/issues/403
        // Enable this test once lotus starts supporting this.
        // RpcTest::identity(StateDecodeParams::request((
        //     MINER_ADDRESS,
        //     Method::MaxTerminationFeeExported as u64,
        //     to_vec(&miner_max_termination_fee_params)?,
        //     tipset.key().into(),
        // ))?),
        // TODO(go-state-types): https://github.com/filecoin-project/go-state-types/issues/403
        // Enable this test once lotus starts supporting this.
        // RpcTest::identity(StateDecodeParams::request((
        //     MINER_ADDRESS,
        //     Method::InitialPledgeExported as u64,
        //     vec![],
        //     tipset.key().into(),
        // ))?),
    ])
}
