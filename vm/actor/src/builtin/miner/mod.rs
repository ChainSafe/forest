// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod deadlines;
mod policy;
mod state;
mod types;

pub use self::deadlines::*;
pub use self::policy::*;
pub use self::state::*;
pub use self::types::SectorOnChainInfo;
pub use self::types::*;

use crate::account::Method as AccountMethod;
use crate::market::{
    ComputeDataCommitmentParams, Method as MarketMethod, OnMinerSectorsTerminateParams,
    VerifyDealsOnSectorProveCommitParams, VerifyDealsOnSectorProveCommitReturn,
};
use crate::power::{
    EnrollCronEventParams, Method as PowerMethod, OnFaultBeginParams, OnFaultEndParams,
    OnSectorModifyWeightDescParams, OnSectorProveCommitParams, OnSectorTerminateParams,
    SectorStorageWeightDesc, SectorTermination, SECTOR_TERMINATION_EXPIRED,
    SECTOR_TERMINATION_FAULTY, SECTOR_TERMINATION_MANUAL,
};
use crate::{
    check_empty_params, is_principal, make_map, ACCOUNT_ACTOR_CODE_ID, BURNT_FUNDS_ACTOR_ADDR,
    CALLER_TYPES_SIGNABLE, INIT_ACTOR_ADDR, REWARD_ACTOR_ADDR, STORAGE_MARKET_ACTOR_ADDR,
    STORAGE_POWER_ACTOR_ADDR,
};
use address::{Address, Payload, Protocol};
use ahash::AHashSet;
use bitfield::BitField;
use byteorder::{BigEndian, ByteOrder};
use cid::{multihash::Blake2b256, Cid};
use clock::ChainEpoch;
use crypto::DomainSeparationTag::{
    InteractiveSealChallengeSeed, SealRandomness, WindowedPoStChallengeSeed,
};
use encoding::Cbor;
use fil_types::{
    InteractiveSealRandomness, PoStProof, PoStRandomness, RegisteredSealProof,
    SealRandomness as SealRandom, SealVerifyInfo, SealVerifyParams, SectorID, SectorNumber,
    SectorSize, WindowPoStVerifyInfo,
};
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser::{BigIntDe, BigIntSer};
use num_bigint::BigInt;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Zero};
use runtime::{ActorCode, Runtime};
use std::collections::HashMap;
use std::error::Error as StdError;
use std::ops::Neg;
use vm::{
    ActorError, DealID, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR,
    METHOD_SEND,
};

// * Updated to specs-actors commit: 9e8c0d1c40d8b41de5dc727b6791c89e14fea4a8

/// Storage Miner actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    ControlAddresses = 2,
    ChangeWorkerAddress = 3,
    ChangePeerID = 4,
    SubmitWindowedPoSt = 5,
    PreCommitSector = 6,
    ProveCommitSector = 7,
    ExtendSectorExpiration = 8,
    TerminateSectors = 9,
    DeclareFaults = 10,
    DeclareFaultsRecovered = 11,
    OnDeferredCronEvent = 12,
    CheckSectorProven = 13,
    AddLockedFund = 14,
    ReportConsensusFault = 15,
    WithdrawBalance = 16,
    ConfirmSectorProofsValid = 17,
    ChangeMultiaddrs = 18,
}

/// Miner Actor
pub struct Actor;

impl Actor {
    pub fn constructor<BS, RT>(
        rt: &mut RT,
        params: MinerConstructorParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*INIT_ACTOR_ADDR))?;

        if !check_supported_proof_types(params.seal_proof_type) {
            return Err(ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "proof type {:?} not allowed for new miner actors",
                    params.seal_proof_type
                ),
            ));
        }

        let owner = resolve_owner_address(rt, params.owner)?;
        let worker = resolve_worker_address(rt, params.worker)?;

        let empty_map = make_map(rt.store()).flush().map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("failed to construct miner state: {}", e),
            )
        })?;

        let empty_array = Amt::<Cid, BS>::new(rt.store()).flush().map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("failed to construct miner state: {}", e),
            )
        })?;

        let empty_deadlines_cid = rt.store().put(&Deadlines::new(), Blake2b256).unwrap();

        let current_epoch = rt.curr_epoch();
        let blake2b = |b: &[u8]| rt.syscalls().hash_blake2b(b);
        let offset = assign_proving_period_offset(*rt.message().receiver(), current_epoch, blake2b)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrSerialization,
                    format!("failed to assign proving period offset: {}", e),
                )
            })?;

        let period_start = next_proving_period_start(current_epoch, offset);
        assert!(period_start > current_epoch);

        let st = State::new(
            empty_array,
            empty_map,
            empty_deadlines_cid,
            owner,
            worker,
            params.peer_id,
            params.multi_address,
            params.seal_proof_type,
            period_start,
        )
        .map_err(|e| ActorError::new(ExitCode::ErrIllegalArgument, e))?;
        rt.create(&st)?;

        // Register cron callback for epoch before the first proving period starts.
        enroll_cron_event(
            rt,
            period_start - 1,
            CronEventPayload {
                event_type: CRON_EVENT_PROVING_PERIOD,
                sectors: Default::default(),
            },
        )?;

        Ok(())
    }

    fn control_addresses<BS, RT>(rt: &mut RT) -> Result<GetControlAddressesReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any();
        let st: State = rt.state()?;
        Ok(GetControlAddressesReturn {
            owner: st.info.owner,
            worker: st.info.worker,
        })
    }

    fn change_worker_address<BS, RT>(
        rt: &mut RT,
        params: ChangeWorkerAddressParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let mut effective_epoch = ChainEpoch::default();
        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            rt.validate_immediate_caller_is(std::iter::once(&st.info.owner))?;
            let worker = resolve_worker_address(rt, params.new_worker)?;

            effective_epoch = rt.curr_epoch() + WORKER_KEY_CHANGE_DELAY;

            // This may replace another pending key change.
            st.info.pending_worker_key = Some(WorkerKeyChange {
                new_worker: worker,
                effective_at: effective_epoch,
            });
            Ok(())
        })??;

        let cron_payload = CronEventPayload {
            event_type: CRON_EVENT_WORKER_KEY_CHANGE,
            sectors: None,
        };
        enroll_cron_event(rt, effective_epoch, cron_payload)?;
        Ok(())
    }

    fn change_peer_ids<BS, RT>(rt: &mut RT, params: ChangePeerIDParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            rt.validate_immediate_caller_is(std::iter::once(&st.info.worker))?;
            st.info.peer_id = params.new_id;
            Ok(())
        })??;
        Ok(())
    }

    fn change_multi_address<BS, RT>(
        rt: &mut RT,
        params: ChangeMultiaddrsParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            rt.validate_immediate_caller_is(std::iter::once(&st.info.worker))?;
            st.info.multi_address = params.new_multi_addrs;
            Ok(())
        })??;
        Ok(())
    }

    /// Invoked by miner's worker address to submit their fallback post
    fn submit_windowed_post<BS, RT>(
        rt: &mut RT,
        params: SubmitWindowedPoStParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let current_epoch = rt.curr_epoch();
        let mut detected_faults_sector: Vec<SectorOnChainInfo> = Vec::new();
        let mut recovered_sectors: Vec<SectorOnChainInfo> = Vec::new();
        let mut penalty = TokenAmount::default();

        let sec_size =
            rt.transaction::<State, Result<SectorSize, ActorError>, _>(|st: &mut State, rt| {
                rt.validate_immediate_caller_is(std::iter::once(&st.info.worker))?;

                let partition_size = st.info.window_post_partition_sectors;
                let submission_partition_limit = window_post_message_partitions_max(partition_size);
                if params.partitions.len() as u64 > submission_partition_limit {
                    return Err(ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        format!(
                            "too many partitions {}, limit {}",
                            params.partitions.len(),
                            submission_partition_limit
                        ),
                    ));
                }
                let deadline = st.deadline_info(current_epoch);
                let mut deadlines = st.load_deadlines(rt.store()).map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to load deadlines: {}", e),
                    )
                })?;

                // Traverse earlier submissions and enact detected faults.
                // This isn't strictly necessary, but keeps the power table up to date eagerly and can force payment
                // of penalties if locked pledge drops too low.
                let (detected_faults, p) =
                    detect_faults_this_period(rt, st, rt.store(), &deadline, &mut deadlines)?;
                detected_faults_sector = detected_faults;
                penalty = p;

                if !deadline.period_started() {
                    return Err(ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        format!(
                            "proving period {} not yet open at {}",
                            deadline.period_start, current_epoch
                        ),
                    ));
                }

                if params.deadline != deadline.index as u64 {
                    return Err(ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        format!(
                            "invalid deadline {} at epoch {}, expected {}",
                            params.deadline, current_epoch, deadline.index
                        ),
                    ));
                }

                // Verify locked funds are are at least the sum of sector initial pledges.
                // Note that this call does not actually compute recent vesting, so the reported locked funds may be
                // slightly higher than the true amount (i.e. slightly in the miner's favour).
                // Computing vesting here would be almost always redundant since vesting is quantized to ~daily units.
                // Vesting will be at most one proving period old if computed in the cron callback.
                verify_pledge_meets_initial_requirements(rt, &st);

                // TODO WPOST (follow-up): process Skipped as faults

                // Work out which sectors are due in the declared partitions at this deadline.
                let partitions_sectors = compute_partitions_sector(
                    &mut deadlines,
                    partition_size,
                    deadline.index,
                    &params.partitions,
                )
                .map_err(|_| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!(
                            "failed to compute partitions sectors at deadline {}, partitions {:?}",
                            deadline.index, params.partitions
                        ),
                    )
                })?;

                let proven_sectors = BitField::union(&partitions_sectors);

                let (sector_infos, declared_recoveries) = st
                    .load_sector_infos_for_proof(rt.store(), proven_sectors)
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to load proven sector info: {}", e),
                        )
                    })?;

                // Verify the proof.
                // A failed verification doesn't immediately cause a penalty; the miner can try again.
                verify_windowed_post(rt, deadline.challenge, &sector_infos, params.proofs.clone())?;

                // Record the successful submission
                let posted_partitions: BitField =
                    params.partitions.iter().map(|&i| i as usize).collect();
                let contains = st.post_submissions.contains_any(&posted_partitions);
                if contains {
                    return Err(ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        "duplicate PoSt partition".to_string(),
                    ));
                }
                st.add_post_submissions(posted_partitions).map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!(
                            "failed to record submissions for partitions: {:?}, {}",
                            params.partitions, e
                        ),
                    )
                })?;

                // If the PoSt was successful, the declared recoveries should be restored
                st.remove_faults(rt.store(), &declared_recoveries)
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to remove recoveries from faults: {}", e),
                        )
                    })?;

                st.remove_recoveries(&declared_recoveries).map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to remove recoveries: {}", e),
                    )
                })?;

                // Load info for recovered sectors for recovery of power outside this state transaction.
                if declared_recoveries.is_empty() {
                    Ok(st.info.sector_size)
                } else {
                    let mut sectors_by_number: HashMap<SectorNumber, SectorOnChainInfo> =
                        HashMap::new();
                    for sec in sector_infos {
                        sectors_by_number.insert(sec.info.sector_number, sec);
                    }
                    declared_recoveries.iter().for_each(|i| {
                        let key = i as u64;
                        let s = sectors_by_number.get(&key).cloned().unwrap();
                        recovered_sectors.push(s);
                    });
                    Ok(st.info.sector_size)
                }
            })??;
        // Remove power for new faults, and burn penalties.
        request_begin_faults(rt, sec_size, &detected_faults_sector)?;
        burn_funds_and_notify_pledge_change(rt, &penalty)?;

        // restore power for recovered sectors
        if !recovered_sectors.is_empty() {
            request_end_faults(rt, sec_size, &recovered_sectors)?;
        }
        Ok(())
    }

    /// Proposals must be posted on chain via sma.PublishStorageDeals before PreCommitSector.
    /// Optimization: PreCommitSector could contain a list of deals that are not published yet.
    fn pre_commit_sector<BS, RT>(rt: &mut RT, params: SectorPreCommitInfo) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if params.expiration <= rt.curr_epoch() {
            return Err(ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "sector expiration {} must be after now {}",
                    params.expiration,
                    rt.curr_epoch()
                ),
            ));
        }
        if params.seal_rand_epoch >= rt.curr_epoch() {
            return Err(ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "seal challenge epoch {} must be before now {}",
                    params.seal_rand_epoch,
                    rt.curr_epoch()
                ),
            ));
        }
        let challenge_earliest = seal_challenge_earliest(rt.curr_epoch(), params.registered_proof);
        if params.seal_rand_epoch < challenge_earliest {
            // The subsequent commitment proof can't possibly be accepted because the seal challenge will be deemed
            // too old. Note that passing this check doesn't guarantee the proof will be soon enough, depending on
            // when it arrives.
            return Err(ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "seal challenge epoch {} too old, must be after {}",
                    params.seal_rand_epoch, challenge_earliest
                ),
            ));
        }

        let newly_vested_amount: TokenAmount = rt.transaction(|st: &mut State, rt| {
            rt.validate_immediate_caller_is(std::iter::once(&st.info.worker))?;
            if params.registered_proof != st.info.seal_proof_type {
                return Err(ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("wrong proof type {:?}", params.registered_proof),
                ));
            };
            let sec = st
                .get_precommitted_sector(rt.store(), params.sector_number)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!(
                            "failed to check precommitted sector: {}, {}",
                            params.sector_number, e
                        ),
                    )
                })?;
            if sec.is_some() {
                // Sector is currently precommitted but still not proven.
                return Err(ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("sector {} already precommitted", params.sector_number),
                ));
            };

            if let Some(sector_info) =
                st.get_sector(rt.store(), params.sector_number)
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to check sector: {}, {}", params.sector_number, e),
                        )
                    })?
            {
                if !sector_info.info.deal_ids.is_empty() {
                    // Sector has been previously committed and proven with deals.
                    return Err(ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        format!(
                            "sector already committed with deals: {}",
                            params.sector_number
                        ),
                    ));
                } else {
                    // Committed Capacity sector upgrade.
                    if params.expiration < sector_info.info.expiration {
                        return Err(ActorError::new(
                            ExitCode::ErrIllegalArgument,
                            format!(
                                "upgraded sector {} expires before original expiration",
                                params.sector_number
                            ),
                        ));
                    }
                }
            };

            validate_expiration(&st, params.expiration)?;

            // TODO revisit if changed in spec, they are ignoring error
            let newly_vested_amount = st
                .unlock_vested_funds(rt.store(), rt.curr_epoch())
                .unwrap_or_default();
            let available_balance = st.get_available_balance(&rt.current_balance()?);
            let deposit_req =
                precommit_deposit(*st.get_sector_size(), params.expiration - rt.curr_epoch());
            if available_balance < deposit_req {
                return Err(ActorError::new(
                    ExitCode::ErrInsufficientFunds,
                    format!("insufficient funds for pre-commit deposit: {}", deposit_req),
                ));
            }
            st.add_pre_commit_deposit(&deposit_req);
            st.assert_balance_invariants(&rt.current_balance()?);
            st.put_precommitted_sector(
                rt.store(),
                SectorPreCommitOnChainInfo {
                    info: params.clone(),
                    pre_commit_deposit: deposit_req,
                    pre_commit_epoch: rt.curr_epoch(),
                },
            )
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!(
                        "failed to write pre-committed sector: {}, {}",
                        params.sector_number, e
                    ),
                )
            })?;
            Ok(newly_vested_amount)
        })??;

        notify_pledge_change(rt, &newly_vested_amount.neg())?;
        let mut bf = BitField::new();
        bf.set(params.sector_number as usize);

        // Request deferred Cron check for PreCommit expiry check.
        let cron_payload = CronEventPayload {
            event_type: CRON_EVENT_PRE_COMMIT_EXPIRY,
            sectors: Some(bf),
        };

        let msd = max_seal_duration(params.registered_proof).ok_or_else(|| {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "no max seal duration set for proof type: {:?}",
                    params.registered_proof
                ),
            )
        })?;
        let expiry_bound = rt.curr_epoch() + msd + 1;
        enroll_cron_event(rt, expiry_bound, cron_payload)?;

        Ok(())
    }

    fn prove_commit_sector<BS, RT>(
        rt: &mut RT,
        params: ProveCommitSectorParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any();

        let sector_number = params.sector_number;
        let st: State = rt.state()?;

        let precommit = st
            .get_precommitted_sector(rt.store(), sector_number)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!(
                        "failed to get precommitted sector: {}, {}",
                        sector_number, e
                    ),
                )
            })?
            .ok_or_else(|| {
                ActorError::new(
                    ExitCode::ErrNotFound,
                    format!("no precommitted sector: {}", sector_number),
                )
            })?;

        let msd = max_seal_duration(precommit.info.registered_proof).ok_or_else(|| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!(
                    "no max seal duration set for proof type: {:?}",
                    precommit.info.registered_proof
                ),
            )
        })?;
        let prove_commit_due = precommit.pre_commit_epoch + msd;
        if rt.curr_epoch() > prove_commit_due {
            return Err(ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "commitment proof for {} too late at {}, due {}",
                    sector_number,
                    rt.curr_epoch(),
                    prove_commit_due
                ),
            ));
        }

        // will abort if seal invalid get_verify_info
        let svi = get_verify_info(
            rt,
            SealVerifyParams {
                sealed_cid: precommit.info.sealed_cid.clone(),
                interactive_epoch: precommit.pre_commit_epoch + PRE_COMMIT_CHALLENGE_DELAY,
                seal_rand_epoch: precommit.info.seal_rand_epoch,
                proof: params.proof,
                deal_ids: precommit.info.deal_ids.clone(),
                sector_num: precommit.info.sector_number,
                registered_proof: precommit.info.registered_proof,
            },
        )?;

        rt.send(
            &*STORAGE_POWER_ACTOR_ADDR,
            PowerMethod::SubmitPoRepForBulkVerify as u64,
            &Serialized::serialize(&svi)?,
            &BigInt::zero(),
        )?;

        Ok(())
    }

    fn confirm_sector_proofs_valid<BS, RT>(
        rt: &mut RT,
        params: ConfirmSectorProofsParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*STORAGE_POWER_ACTOR_ADDR))?;

        let st: State = rt.state()?;

        for num in params.sectors {
            let precommit = st
                .get_precommitted_sector(rt.store(), num)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to get precommitted sector: {}, {}", num, e),
                    )
                })?
                .ok_or_else(|| {
                    ActorError::new(
                        ExitCode::ErrNotFound,
                        format!("no precommitted sector: {}", num),
                    )
                })?;

            // Check (and activate) storage deals associated to sector. Abort if checks failed.
            // return DealWeight for the deal set in the sector
            let ser_params = Serialized::serialize(VerifyDealsOnSectorProveCommitParams {
                deal_ids: precommit.info.deal_ids.clone(),
                sector_expiry: precommit.info.expiration,
            })?;

            // TODO revisit spec TODOs
            let mut ret = rt.send(
                &*STORAGE_MARKET_ACTOR_ADDR,
                MarketMethod::VerifyDealsOnSectorProveCommit as u64,
                &ser_params,
                &TokenAmount::zero(),
            )?;
            let deal_weights: VerifyDealsOnSectorProveCommitReturn = ret.deserialize()?;

            // Request power for activated sector.
            // Return initial pledge requirement.
            let param = Serialized::serialize(OnSectorProveCommitParams {
                weight: SectorStorageWeightDesc {
                    sector_size: st.info.sector_size,
                    deal_weight: deal_weights.deal_weight.clone(),
                    verified_deal_weight: deal_weights.verified_deal_weight.clone(),
                    duration: precommit.info.expiration - rt.curr_epoch(),
                },
            })?;
            ret = rt.send(
                &*STORAGE_POWER_ACTOR_ADDR,
                PowerMethod::OnSectorProveCommit as u64,
                &param,
                &TokenAmount::zero(),
            )?;
            let BigIntDe(initial_pledge) = ret.deserialize()?;

            // Add sector and pledge lock-up to miner state
            let current_epoch = rt.curr_epoch();
            let expired_epoch = precommit.info.expiration;
            let info = precommit.info;
            let deposit = precommit.pre_commit_deposit;

            let vested_amount =
            rt.transaction::<State, Result<TokenAmount, ActorError>, _>(|st, rt| {
                let newly_vested_fund = st.unlock_vested_funds(rt.store(), current_epoch).map_err(|e| {
                    ActorError::new(ExitCode::ErrIllegalState, format!("failed to vest new funds: {}", e))
                })?;

                // unlock deposit for successful proof, make it available for lock-up as initial pledge
                st.subtract_pre_commit_deposit(&deposit);

                // Verify locked funds are are at least the sum of sector initial pledges.
                verify_pledge_meets_initial_requirements(rt, st);

                // lock up initial pledge for new sector
                let available_balance = st.get_available_balance(&rt.current_balance()?);
                if available_balance < initial_pledge {
                    return Err(ActorError::new(ExitCode::ErrInsufficientFunds, format!("insufficient funds for initial pledge requirement {}, available: {}", initial_pledge, available_balance)));
                }

                st.add_locked_funds(rt.store(), current_epoch, &initial_pledge, PLEDGE_VESTING_SPEC).map_err(|e| {
                    ActorError::new(ExitCode::ErrIllegalState, format!("failed to add pledge: {}", e))
                })?;

                st.assert_balance_invariants(&rt.current_balance()?);

                let new_sector_info = SectorOnChainInfo{
                    info,
                    activation_epoch: current_epoch,
                    deal_weight: deal_weights.deal_weight,
                    verified_deal_weight: deal_weights.verified_deal_weight
                };

                st.put_sector(rt.store(), new_sector_info).map_err(|e| {
                    ActorError::new(ExitCode::ErrIllegalState, format!("failed to prove commit: {}", e))
                })?;

                st.delete_precommitted_sector(rt.store(), num).map_err(|e| {
                    ActorError::new(ExitCode::ErrIllegalState, format!("failed to delete precommit for sector {}: {}", num, e))
                })?;

                st.add_sector_expirations(rt.store(), expired_epoch, &[num]).map_err(|e| {
                    ActorError::new(ExitCode::ErrIllegalState, format!("failed to add new sector {} expiration: {}", num, e))
                })?;

                // Add to new sectors, a staging ground before scheduling to a deadline at end of proving period.
                st.add_new_sectors(&[num]).map_err(|e| {
                    ActorError::new(ExitCode::ErrIllegalState, format!("failed to add new sector number {}: {}", num, e))
                })?;

                Ok(newly_vested_fund)
            })??;

            notify_pledge_change(rt, &(initial_pledge - vested_amount))?;
        }
        Ok(())
    }

    fn check_sector_proven<BS, RT>(
        rt: &mut RT,
        params: CheckSectorProvenParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any();
        let st: State = rt.state()?;

        let sec = st.get_sector(rt.store(), params.sector_number);
        if let Ok(None) = sec {
            ActorError::new(
                ExitCode::ErrNotFound,
                format!("sector hasn't been proven {}", params.sector_number),
            );
        }

        Ok(())
    }

    fn extend_sector_expiration<BS, RT>(
        rt: &mut RT,
        params: ExtendSectorExpirationParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let st: State = rt.state()?;
        rt.validate_immediate_caller_is(std::iter::once(&st.info.worker))?;
        let sector_number = params.sector_number;

        validate_expiration(&st, params.new_expiration)?;

        let mut sector = st
            .get_sector(rt.store(), sector_number)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to add load sector {}: {}", sector_number, e),
                )
            })?
            .ok_or_else(|| {
                ActorError::new(
                    ExitCode::ErrNotFound,
                    format!("no such sector {}", sector_number),
                )
            })?;

        let old_expiration = sector.info.expiration;
        let storage_weight_desc_prev = to_storage_weight_desc(st.info.sector_size, &sector);
        let extension_len = params.new_expiration - old_expiration;

        if extension_len < 0 {
            return Err(ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!("cannot reduce sector expiration {}", extension_len),
            ));
        }

        let mut storage_weight_desc_new = storage_weight_desc_prev.clone();
        storage_weight_desc_new.duration = storage_weight_desc_prev.duration + extension_len;

        let ser_params = Serialized::serialize(OnSectorModifyWeightDescParams {
            prev_weight: storage_weight_desc_prev,
            new_weight: storage_weight_desc_new,
        })?;

        rt.send(
            &*STORAGE_POWER_ACTOR_ADDR,
            PowerMethod::OnSectorModifyWeightDesc as u64,
            &ser_params,
            &BigInt::zero(),
        )?;

        // store new sector expiry
        rt.transaction(|st: &mut State, rt| {
            sector.info.expiration = params.new_expiration;
            st.put_sector(rt.store(), sector.clone()).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("Failed to update sector: {:?}, {}", sector, e),
                )
            })?;

            // move expiration from old epoch to new
            st.remove_sector_expirations(rt.store(), old_expiration, &[params.sector_number])
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!(
                            "failed to update sector expiration: {:?}, {}",
                            sector_number, e
                        ),
                    )
                })?;
            st.add_sector_expirations(rt.store(), params.new_expiration, &[params.sector_number])
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!(
                            "failed to update sector expiration: {:?}, {}",
                            sector_number, e
                        ),
                    )
                })?;

            Ok(())
        })?
    }

    fn terminate_sectors<BS, RT>(
        rt: &mut RT,
        params: TerminateSectorsParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let st: State = rt.state()?;
        rt.validate_immediate_caller_is(std::iter::once(&st.info.worker))?;

        // Note: this cannot terminate pre-committed but un-proven sectors.
        // They must be allowed to expire (and deposit burnt).
        terminate_sectors(rt, &params.sectors, SECTOR_TERMINATION_MANUAL)?;
        Ok(())
    }

    fn declare_faults<BS, RT>(rt: &mut RT, params: DeclareFaultsParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if params.faults.len() > WPOST_PERIOD_DEADLINES {
            return Err(ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "too many declarations {}, max {}",
                    params.faults.len(),
                    WPOST_PERIOD_DEADLINES
                ),
            ));
        }
        let current_epoch = rt.curr_epoch();
        let mut declared_fault_sectors: Vec<SectorOnChainInfo> = Vec::new();
        let mut detected_fault_sectors: Vec<SectorOnChainInfo> = Vec::new();

        let (penalty, sector_size) = rt.transaction(|st: &mut State, rt| {
            rt.validate_immediate_caller_is(std::iter::once(&st.info.worker))?;

            let current_deadline = st.deadline_info(current_epoch);
            let mut deadlines = st.load_deadlines(rt.store()).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to load deadlines: {}", e),
                )
            })?;

            // Traverse earlier submissions and enact detected faults.
            // This is necessary to move the NextDeadlineToProcessFaults index past the deadline that this recovery
            // is targeting, so that the recovery won't be declared failed next time it's checked during this proving period.
            let (detected_faults, mut penalty) =
                detect_faults_this_period(rt, st, rt.store(), &current_deadline, &mut deadlines)?;
            detected_fault_sectors = detected_faults;

            let declared_sectors = params
                .faults
                .into_iter()
                .map(|decl| {
                    let target_deadline: DeadlineInfo = declaration_deadline_info(
                        st.proving_period_start,
                        decl.deadline as usize,
                        current_epoch,
                    )
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalArgument,
                            format!("invalid fault declaration deadline: {}", e),
                        )
                    })?;
                    validate_fr_declaration(&mut deadlines, &target_deadline, &decl.sectors)
                        .map_err(|e| {
                            ActorError::new(
                                ExitCode::ErrIllegalArgument,
                                format!("invalid fault declaration: {}", e),
                            )
                        })?;
                    Ok(decl.sectors)
                })
                .collect::<Result<Vec<BitField>, ActorError>>()?;

            let all_declared = BitField::union(&declared_sectors);

            // Split declarations into declarations of new faults, and retraction of declared recoveries.
            let recoveries = &st.recoveries & &all_declared;
            let new_faults = &all_declared - &recoveries;

            if !new_faults.is_empty() {
                // check new fault are really new
                if st.faults.contains_any(&new_faults) {
                    // This could happen if attempting to declare a fault for a deadline that's already passed,
                    // detected and added to Faults above.
                    // The miner must for the fault detection at proving period end, or submit again omitting
                    // sectors in deadlines that have passed.
                    // Alternatively, we could subtract the just-detected faults from new faults.
                    return Err(ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        "attempted to re-declare fault".to_string(),
                    ));
                }

                // Add new faults to state and charge fee.
                // Note: this sets the fault epoch for all declarations to be the beginning of this proving period,
                // even if some sectors have already been proven in this period.
                // It would better to use the target deadline's proving period start (which may be the one subsequent
                // to the current).
                st.add_faults(rt.store(), &new_faults, st.proving_period_start)
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to add faults: {}", e),
                        )
                    })?;
                // Note: this charges a fee for all declarations, even if the sectors have already been proven
                // in this proving period. This discourages early declaration compared with waiting for
                // the proving period to roll over.
                // It would be better to charge a fee for this proving period only if the target deadline has
                // not already passed. If it _has_ already passed then either:
                // - the miner submitted PoSt successfully and should not be penalised more relative to
                //   submitting this declaration after the proving period rolls over, or
                // - the miner failed to submit PoSt and will be penalised at the proving period end
                // In either case, the miner will pay a fee for the subsequent proving period at the start
                // of that period, unless faults are recovered sooner.

                // Load info for sectors.
                let declared_fault_sectors =
                    st.load_sector_infos(rt.store(), &new_faults).map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to load fault sectors: {}", e),
                        )
                    })?;

                // Unlock penalty for declared faults.
                let declared_penalty = unlock_penalty(
                    st,
                    rt.store(),
                    current_epoch,
                    &declared_fault_sectors,
                    &pledge_penalty_for_sector_declared_fault,
                )
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to charge fault fee: {}", e),
                    )
                })?;
                penalty += declared_penalty;

                if !recoveries.is_empty() {
                    st.remove_recoveries(&recoveries).map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to remove recoveries: {}", e),
                        )
                    })?;
                }
            }

            Ok((penalty, st.info.sector_size))
        })??;

        // remove power for new faulty sectors
        detected_fault_sectors.append(&mut declared_fault_sectors);
        request_begin_faults(rt, sector_size, &detected_fault_sectors)?;
        burn_funds_and_notify_pledge_change(rt, &penalty)?;

        Ok(())
    }

    fn declare_faults_recovered<BS, RT>(
        rt: &mut RT,
        params: DeclareFaultsRecoveredParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if params.recoveries.len() > WPOST_PERIOD_DEADLINES {
            return Err(ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "too many declarations {}, max {}",
                    params.recoveries.len(),
                    WPOST_PERIOD_DEADLINES
                ),
            ));
        }
        let mut detected_fault_sectors: Vec<SectorOnChainInfo> = Vec::new();
        let current_epoch = rt.curr_epoch();

        let (penalty, sector_size) = rt.transaction(|st: &mut State, rt| {
            rt.validate_immediate_caller_is(std::iter::once(&st.info.worker))?;

            let current_deadline = st.deadline_info(current_epoch);
            let mut deadlines = st.load_deadlines(rt.store()).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to load deadlines: {}", e),
                )
            })?;

            // Traverse earlier submissions and enact detected faults.
            // This is necessary to move the NextDeadlineToProcessFaults index past the deadline that this recovery
            // is targeting, so that the recovery won't be declared failed next time it's checked during this proving period.
            let (fault_sectors, penalty) =
                detect_faults_this_period(rt, st, rt.store(), &current_deadline, &mut deadlines)?;
            detected_fault_sectors = fault_sectors;

            let declared_sectors = params
                .recoveries
                .into_iter()
                .map(|decl| {
                    let target_deadline = declaration_deadline_info(
                        st.proving_period_start,
                        decl.deadline as usize,
                        current_epoch,
                    )
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalArgument,
                            format!("invalid recovery declaration deadline: {}", e),
                        )
                    })?;

                    validate_fr_declaration(&mut deadlines, &target_deadline, &decl.sectors)
                        .map_err(|e| {
                            ActorError::new(
                                ExitCode::ErrIllegalArgument,
                                format!("invalid recovery declaration: {}", e),
                            )
                        })?;
                    Ok(decl.sectors)
                })
                .collect::<Result<Vec<BitField>, ActorError>>()?;

            let all_recoveries = BitField::union(&declared_sectors);

            if !st.faults.contains_all(&all_recoveries) {
                return Err(ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    "declared recoveries not currently faulty".to_string(),
                ));
            }

            if st.recoveries.contains_any(&all_recoveries) {
                return Err(ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    "sector already declared recovered".to_string(),
                ));
            }

            st.add_recoveries(&all_recoveries).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("invalid recoveries: {}", e),
                )
            })?;

            Ok((penalty, st.info.sector_size))
        })??;

        // remove power for new faulty sectors
        request_begin_faults(rt, sector_size, &detected_fault_sectors)?;
        burn_funds_and_notify_pledge_change(rt, &penalty)?;

        // Power is not restored yet, but when the recovered sectors are successfully PoSted.
        Ok(())
    }

    /// Locks up some amount of a the miner's unlocked balance (including any received alongside the invoking message).
    fn add_locked_fund<BS, RT>(rt: &mut RT, amount: TokenAmount) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let vested_amount = rt.transaction(|st: &mut State, rt| {
            rt.validate_immediate_caller_is(&[st.info.worker, st.info.owner, *REWARD_ACTOR_ADDR])?;

            let newly_vested_amount = st
                .unlock_vested_funds(rt.store(), rt.curr_epoch())
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to vest funds: {}", e),
                    )
                })?;
            let available_balance = st.get_available_balance(&rt.current_balance()?);
            if available_balance < amount {
                return Err(ActorError::new(
                    ExitCode::ErrInsufficientFunds,
                    format!(
                        "insufficient funds to lock, available: {}, requested: {}",
                        available_balance, amount
                    ),
                ));
            }

            st.add_locked_funds(rt.store(), rt.curr_epoch(), &amount, PLEDGE_VESTING_SPEC)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to lock pledge: {}", e),
                    )
                })?;
            Ok(newly_vested_amount)
        })??;
        let delta = amount - vested_amount;
        notify_pledge_change(rt, &delta)?;
        Ok(())
    }

    fn report_consensus_fault<BS, RT>(
        rt: &mut RT,
        params: ReportConsensusFaultParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // Note: only the first reporter of any fault is rewarded.
        // Subsequent invocations fail because the target miner has been removed.
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;
        let reporter = *rt.message().caller();

        let fault = rt
            .syscalls()
            .verify_consensus_fault(&params.header1, &params.header2, &params.header_extra)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("fault not verified: {}", e),
                )
            })?
            .ok_or_else(|| {
                ActorError::new(ExitCode::ErrIllegalArgument, "Invalid fault".to_string())
            })?;

        // Elapsed since the fault (i.e. since the higher of the two blocks)
        let fault_age = rt.curr_epoch() - fault.epoch;
        if fault_age <= 0 {
            return Err(ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "invalid fault epoch {} ahead of current {}",
                    fault.epoch,
                    rt.curr_epoch()
                ),
            ));
        }

        let st: State = rt.state()?;

        rt.send(
            &*STORAGE_POWER_ACTOR_ADDR,
            PowerMethod::OnConsensusFault as u64,
            &Serialized::serialize(BigIntSer(&st.locked_funds))?,
            &BigInt::zero(),
        )?;

        // TODO: terminate deals with market actor, https://github.com/filecoin-project/specs-actors/issues/279

        // Reward reporter with a share of the miner's current balance.
        let slasher_reward = reward_for_consensus_slash_report(fault_age, rt.current_balance()?);
        rt.send(
            &reporter,
            METHOD_SEND,
            &Serialized::default(),
            &slasher_reward,
        )?;

        // Delete the actor and burn all remaining funds
        rt.delete_actor(&*BURNT_FUNDS_ACTOR_ADDR)?;

        Ok(())
    }

    fn withdraw_balance<BS, RT>(
        rt: &mut RT,
        params: WithdrawBalanceParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO negative amount requested will have inconsistent exit code
        // (we throw serialization error, will checked a signed integer and throw illegal argument)
        let st: State = rt.state()?;
        let vested_amount =
            rt.transaction::<State, Result<TokenAmount, ActorError>, _>(|st, rt| {
                rt.validate_immediate_caller_is(std::iter::once(&st.info.owner))?;
                let newly_vested_amount = st
                    .unlock_vested_funds(rt.store(), rt.curr_epoch())
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("Failed to vest funds {:}", e),
                        )
                    })?;

                Ok(newly_vested_amount)
            })??;

        let curr_balance = rt.current_balance()?;
        let amount_withdrawn = std::cmp::min(
            st.get_available_balance(&curr_balance),
            params.amount_requested,
        );
        assert!(amount_withdrawn <= curr_balance);

        rt.send(
            &st.info.owner,
            METHOD_SEND,
            &Serialized::default(),
            &amount_withdrawn,
        )?;

        notify_pledge_change(rt, &vested_amount.neg())?;

        st.assert_balance_invariants(&rt.current_balance()?);
        Ok(())
    }

    fn on_deferred_cron_event<BS, RT>(
        rt: &mut RT,
        payload: CronEventPayload,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match payload.event_type {
            CRON_EVENT_PROVING_PERIOD => handle_proving_period(rt)?,
            CRON_EVENT_PRE_COMMIT_EXPIRY => check_precommit_expiry(rt, &payload.sectors)?,
            CRON_EVENT_WORKER_KEY_CHANGE => commit_worker_key_change(rt)?,
            _ => (),
        };

        Ok(())
    }
}

/// Invoked at the end of each proving period, at the end of the epoch before the next one starts.
fn handle_proving_period<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    // Vest locked funds.
    // This happens first so that any subsequent penalties are taken from locked pledge, rather than free funds.
    let vested_amount =
        rt.transaction::<State, Result<TokenAmount, ActorError>, _>(|st, rt| {
            let newly_vested_fund = st
                .unlock_vested_funds(rt.store(), rt.curr_epoch())
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to vest funds {:}", e),
                    )
                })?;
            Ok(newly_vested_fund)
        })??;

    notify_pledge_change(rt, &vested_amount.neg())?;

    // Note: because the cron actor is not invoked on epochs with empty tipsets, the current epoch is not necessarily
    // exactly the final epoch of the period; it may be slightly later (i.e. in the subsequent period).
    // Further, this method is invoked once *before* the first proving period starts, after the actor is first
    // constructed; this is detected by !deadline.PeriodStarted().
    // Use deadline.PeriodEnd() rather than rt.CurrEpoch unless certain of the desired semantics.

    let deadline = {
        // Detect and penalize missing proofs.
        let mut detected_fault_sectors: Vec<SectorOnChainInfo> = Vec::new();
        let curr_epoch = rt.curr_epoch();
        let mut penalty = TokenAmount::default();
        let (sector_size, deadline) =
            rt.transaction::<State, Result<_, ActorError>, _>(|st: &mut State, rt| {
                let deadline = st.deadline_info(curr_epoch);

                if deadline.period_started() {
                    // Skip checking faults on the first, incomplete period.
                    let mut deadlines = st.load_deadlines(rt.store()).map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to load deadlines {:}", e),
                        )
                    })?;
                    let (detected_faults, p) = process_missing_post_faults(
                        rt,
                        st,
                        rt.store(),
                        &mut deadlines,
                        deadline.period_start,
                        deadline.index,
                        curr_epoch,
                    )?;
                    detected_fault_sectors = detected_faults;
                    penalty = p;
                }
                Ok((st.info.sector_size, deadline))
            })??;

        // Remove power for new faults, and burn penalties.
        request_begin_faults(rt, sector_size, &detected_fault_sectors)?;
        burn_funds_and_notify_pledge_change(rt, &penalty)?;
        deadline
    };

    {
        // Expire sectors that are due.
        let expired_sectors = rt.transaction::<State, Result<_, ActorError>, _>(|st, rt| {
            Ok(
                pop_sector_expirations(st, rt.store(), deadline.period_end()).map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to load expired sectors {:}", e),
                    )
                })?,
            )
        })??;

        // Terminate expired sectors (sends messages to power and market actors).
        terminate_sectors(rt, &expired_sectors, SECTOR_TERMINATION_EXPIRED)?;
    }

    {
        // Terminate sectors with faults that are too old, and pay fees for ongoing faults.
        let (expired_faults, ongoing_fault_penalty) = rt
            .transaction::<State, Result<_, ActorError>, _>(|st, rt| {
                let (expired_faults, ongoing_faults) =
                    pop_expired_faults(st, rt.store(), deadline.period_end() - FAULT_MAX_AGE)
                        .map_err(|e| {
                            ActorError::new(
                                ExitCode::ErrIllegalState,
                                format!("failed to load fault sectors: {}", e),
                            )
                        })?;

                // Load info for ongoing faults.
                // TODO: this is potentially super expensive for a large miner with ongoing faults
                let ongoing_fault_info = st
                    .load_sector_infos(rt.store(), &ongoing_faults)
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to charge fault fee: {}", e),
                        )
                    })?;

                // Unlock penalty for ongoing faults.
                let ongoing_fault_penalty = unlock_penalty(
                    st,
                    rt.store(),
                    deadline.period_end(),
                    &ongoing_fault_info,
                    &pledge_penalty_for_sector_declared_fault,
                )
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to charge fault fee: {}", e),
                    )
                })?;
                Ok((expired_faults, ongoing_fault_penalty))
            })??;

        terminate_sectors(rt, &expired_faults, SECTOR_TERMINATION_FAULTY)?;
        burn_funds_and_notify_pledge_change(rt, &ongoing_fault_penalty)?;
    }

    let proving_period_start = {
        // Establish new proving sets and clear proofs.
        rt.transaction::<State, Result<_, ActorError>, _>(|st, rt| {
            let mut deadlines = st.load_deadlines(rt.store()).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to load deadlines {:}", e),
                )
            })?;

            // assign new sectors to deadlines
            let new_sectors: Vec<_> = st
                .new_sectors
                .bounded_iter(NEW_SECTORS_PER_PERIOD_MAX)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to expand new sectors {:}", e),
                    )
                })?
                .collect();

            if !new_sectors.is_empty() {
                // TODO spec indicates passing in `seed` param, however its currently not being used hence its absence here
                assign_new_sectors(
                    &mut deadlines,
                    st.info.window_post_partition_sectors as usize,
                    &new_sectors,
                )
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to assign new sectors to deadlines: {}", e),
                    )
                })?;

                // store updated deadline state
                st.save_deadlines(rt.store(), deadlines).map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to store new deadlines: {}", e),
                    )
                })?;

                st.new_sectors = BitField::new();
            }

            // Reset PoSt submissions for next period
            st.clear_post_submissions().map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to clear PoSt submissions: {}", e),
                )
            })?;

            // set new proving period start
            if deadline.period_started() {
                st.proving_period_start += WPOST_PROVING_PERIOD;
            }
            Ok(st.proving_period_start)
        })??
    };

    // Schedule cron callback for next period
    let next_period_end = proving_period_start + WPOST_PROVING_PERIOD - 1;
    enroll_cron_event(
        rt,
        next_period_end,
        CronEventPayload {
            event_type: CRON_EVENT_PROVING_PERIOD,
            sectors: Default::default(),
        },
    )?;
    Ok(())
}
/// Detects faults from PoSt submissions that have not arrived in schedule earlier in the current proving period.
fn detect_faults_this_period<BS, RT>(
    rt: &RT,
    st: &mut State,
    store: &BS,
    curr_deadline: &DeadlineInfo,
    deadlines: &mut Deadlines,
) -> Result<(Vec<SectorOnChainInfo>, TokenAmount), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if curr_deadline.period_elapsed() {
        // A cron event has not yet processed the previous proving period and established the next one.
        // This is possible in the first non-empty epoch of a proving period if there was an empty tipset on the
        // last epoch of the previous period.
        // The period must be reset before processing missing-post faults.
        return Err(ActorError::new(
            ExitCode::ErrIllegalState,
            format!(
                "proving period at {} elapsed, next one not yet opened",
                curr_deadline.period_started()
            ),
        ));
    }

    Ok(process_missing_post_faults(
        rt,
        st,
        store,
        deadlines,
        curr_deadline.period_start,
        curr_deadline.index,
        curr_deadline.current_epoch,
    )?)
}

/// Detects faults from missing PoSt submissions that did not arrive by some deadline, and moves
/// the NextDeadlineToProcessFaults index up to that deadline.
fn process_missing_post_faults<BS, RT>(
    _rt: &RT,
    st: &mut State,
    store: &BS,
    deadlines: &mut Deadlines,
    period_start: ChainEpoch,
    before_deadline: usize,
    current_epoch: ChainEpoch,
) -> Result<(Vec<SectorOnChainInfo>, TokenAmount), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    // This method must be called to process the end of one proving period before moving on to the process something
    // the next. Usually, sinceDeadline would be <= beforeDeadline since the former is updated to match the latter
    // and the call during proving period cron will set NextDeadlineToProcessFaults back to zero.
    // In the odd case where the proving period's penultimate tipset is empty, this method must be invoked by the cron
    // callback before allowing any fault/recovery declaration or PoSt to do partial processing.
    assert!(
        st.next_deadline_to_process_faults <= before_deadline,
        format!(
            "invalid next-deadline {} after before-deadline {} while detecting faults",
            st.next_deadline_to_process_faults, before_deadline
        )
    );

    let (detected_faults, failed_recoveries) = compute_faults_from_missing_posts(
        st,
        deadlines,
        st.next_deadline_to_process_faults,
        before_deadline,
    )
    .map_err(|e| {
        ActorError::new(
            ExitCode::ErrIllegalState,
            format!("failed to compute detected faults: {}", e),
        )
    })?;
    st.next_deadline_to_process_faults = before_deadline % WPOST_PERIOD_DEADLINES;

    st.add_faults(store, &detected_faults, period_start)
        .map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("failed to record new faults: {}", e),
            )
        })?;

    st.remove_recoveries(&failed_recoveries).map_err(|e| {
        ActorError::new(
            ExitCode::ErrIllegalState,
            format!("failed to record failed recoveries: {}", e),
        )
    })?;

    // Load info for sectors.
    let mut detected_fault_sectors =
        st.load_sector_infos(store, &detected_faults).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("failed to load fault sectors: {}", e),
            )
        })?;
    let mut failed_recovery_sectors =
        st.load_sector_infos(store, &failed_recoveries)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to load failed recovery sectors: {}", e),
                )
            })?;

    // unlock sector penalty for all undeclared faults
    detected_fault_sectors.append(&mut failed_recovery_sectors);

    let penalty = unlock_penalty(
        st,
        store,
        current_epoch,
        &detected_fault_sectors,
        &pledge_penalty_for_sector_undeclared_fault,
    )
    .map_err(|e| {
        ActorError::new(
            ExitCode::ErrIllegalState,
            format!("failed to charge sector penalty: {}", e),
        )
    })?;

    Ok((detected_fault_sectors, penalty))
}

/// Computes the sectors that were expected to be present in partitions of a PoSt submission but were not, in the
/// deadlines from sinceDeadline (inclusive) to beforeDeadline (exclusive).
fn compute_faults_from_missing_posts(
    st: &mut State,
    deadlines: &mut Deadlines,
    since_deadline: usize,
    before_deadline: usize,
) -> Result<(BitField, BitField), String> {
    // TODO: Iterating this bitfield and keeping track of what partitions we're expecting could remove the
    // need to expand this into a potentially-giant map. But it's tricksy.
    let partition_size = st.info.window_post_partition_sectors;
    let submissions: AHashSet<_> = st
        .post_submissions
        .bounded_iter(active_partitions_max(partition_size))?
        .collect();

    let mut f_groups: Vec<BitField> = Vec::new();
    let mut r_groups: Vec<BitField> = Vec::new();
    let mut deadline_first_partition: u64 = 0;

    // let mut dl_idx = since_deadline;
    for dl_idx in 0..before_deadline {
        let (dl_part_count, dl_sector_count) =
            deadline_count(deadlines, partition_size as usize, dl_idx)?;

        if dl_idx < since_deadline {
            deadline_first_partition += dl_part_count as u64;
            continue;
        }

        let deadline_sectors = deadlines
            .due
            .get(dl_idx)
            .expect("Should be able to index due deadlines");
        for dl_part_idx in 0..dl_part_count {
            if !submissions.contains(&(deadline_first_partition as usize + dl_part_idx)) {
                // no PoSt received in prior period
                let part_first_sector_idx = dl_part_idx * partition_size as usize;
                let part_sector_count = std::cmp::min(
                    partition_size as usize,
                    dl_sector_count - part_first_sector_idx,
                );

                let partition_sectors =
                    deadline_sectors.slice(part_first_sector_idx, part_sector_count)?;

                // record newly-faulty sectors
                let new_faults = &st.faults - &partition_sectors;
                f_groups.push(new_faults);

                // record failed recoveries
                let failed_recovery = &st.recoveries & &partition_sectors;
                r_groups.push(failed_recovery);
            }
        }
        deadline_first_partition += dl_part_count as u64;
    }
    let detected_faults = BitField::union(&f_groups);
    let failed_recoveries = BitField::union(&r_groups);

    Ok((detected_faults, failed_recoveries))
}

// Check expiry is exactly *the epoch before* the start of a proving period.
fn validate_expiration(st: &State, expiration: ChainEpoch) -> Result<(), ActorError> {
    let period_offset = st.proving_period_start % WPOST_PROVING_PERIOD;
    let expiry_offset = (expiration + 1) % WPOST_PROVING_PERIOD;
    if expiry_offset != period_offset {
        return Err(ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("invalid expiration {}, must be immediately before proving period boundary {} mod {}", expiration, period_offset, WPOST_PROVING_PERIOD),
                ));
    }
    Ok(())
}

/// Removes and returns sector numbers that expire at or before an epoch.
fn pop_sector_expirations<BS>(
    st: &mut State,
    store: &BS,
    epoch: ChainEpoch,
) -> Result<BitField, String>
where
    BS: BlockStore,
{
    let mut expired_epochs: Vec<ChainEpoch> = Vec::new();
    let mut expired_sectors: Vec<BitField> = Vec::new();

    st.for_each_sector_expiration(store, |expiry: ChainEpoch, sectors: &BitField| {
        if expiry > epoch {
            return Err("done".to_string());
        }
        expired_epochs.push(expiry);
        expired_sectors.push(sectors.clone());
        Ok(())
    })?;

    st.clear_sector_expirations(store, &expired_epochs)?;

    let all_expiries = BitField::union(&expired_sectors);

    Ok(all_expiries)
}

/// Removes and returns sector numbers that were faulty at or before an epoch, and returns the sector
/// numbers for other ongoing faults.
fn pop_expired_faults<BS>(
    st: &mut State,
    store: &BS,
    latest_termination: ChainEpoch,
) -> Result<(BitField, BitField), String>
where
    BS: BlockStore,
{
    let mut expired_epochs: Vec<ChainEpoch> = Vec::new();
    let mut all_expiries = BitField::new();
    let mut all_ongoing_faults = BitField::new();

    st.for_each_fault_epoch(store, |fault_start: ChainEpoch, faults: &BitField| {
        if fault_start <= latest_termination {
            all_expiries |= faults;
            expired_epochs.push(fault_start);
        } else {
            all_ongoing_faults |= faults;
        }
        Ok(())
    })?;

    st.clear_fault_epochs(store, &expired_epochs)?;

    Ok((all_expiries, all_ongoing_faults))
}

fn check_precommit_expiry<BS, RT>(
    rt: &mut RT,
    optional_sectors: &Option<BitField>,
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    // initialize here to add together for all sectors and minimize calls across actors
    let mut deposit_burn = TokenAmount::default();
    rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
        if let Some(sectors) = optional_sectors {
            sectors
                .iter()
                .try_for_each(|i| {
                    let sec_num = i as u64;
                    let sector = match st.get_precommitted_sector(rt.store(), sec_num)? {
                        Some(sec) => sec,
                        // Already committed/deleted
                        None => return Ok(()),
                    };

                    // delete actor
                    st.delete_precommitted_sector(rt.store(), sec_num)?;

                    // increment deposit to burn
                    deposit_burn += sector.pre_commit_deposit;

                    Ok(())
                })
                .map_err(|e: String| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to check precommit expires: {}", e),
                    )
                })?;
        }
        st.pre_commit_deposit -= &deposit_burn;

        Ok(())
    })??;
    // This deposit was locked separately to pledge collateral so there's no pledge change here.
    burn_funds(rt, &deposit_burn)?;
    Ok(())
}

fn terminate_sectors<BS, RT>(
    rt: &mut RT,
    sector_nos: &BitField,
    termination_type: SectorTermination,
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if sector_nos.is_empty() {
        return Ok(());
    }

    let mut deal_ids: Vec<DealID> = Vec::new();
    let mut all_sectors: Vec<SectorOnChainInfo> = Vec::new();
    let mut faulty_sectors: Vec<SectorOnChainInfo> = Vec::new();
    let mut penalty = TokenAmount::default();
    let state: State = rt.state()?;
    let current_epoch = &rt.curr_epoch();

    rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
        let max_allowed_faults = st.get_max_allowed_faults(rt.store()).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("failed to load fault max: {}", e),
            )
        })?;

        // narrow faults to just the set that are expiring, before expanding to a map
        let faults = &st.faults & sector_nos;

        let faults_map: AHashSet<_> = faults
            .bounded_iter(max_allowed_faults as usize)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to expand faults: {}", e),
                )
            })?
            .map(|i| i as u64)
            .collect();

        sector_nos
            .iter()
            .try_for_each(|i| {
                let i = i as u64;
                let sector = st
                    .get_sector(rt.store(), i)?
                    .ok_or_else(|| format!("no sector found: {}", i))?;

                deal_ids.extend_from_slice(&sector.info.deal_ids);
                let fault = faults_map.contains(&i);
                if fault {
                    faulty_sectors.push(sector.clone());
                }

                all_sectors.push(sector);
                Ok(())
            })
            .map_err(|e: String| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to load sector metadata: {}", e),
                )
            })?;

        let mut deadlines = st.load_deadlines(rt.store()).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("failed to load deadlines: {}", e),
            )
        })?;

        remove_terminated_sectors(st, rt.store(), &mut deadlines, sector_nos).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("failed to store new deadlines: {}", e),
            )
        })?;

        if termination_type != SECTOR_TERMINATION_EXPIRED {
            penalty = unlock_penalty(
                st,
                rt.store(),
                *current_epoch,
                &all_sectors,
                &pledge_penalty_for_sector_termination,
            )
            .unwrap();
        }
        Ok(())
    })??;

    // End any fault state before terminating sector power.
    request_end_faults(rt, state.info.sector_size, &faulty_sectors)?;
    request_terminate_deals(rt, deal_ids)?;
    request_terminate_power(rt, termination_type, state.info.sector_size, &all_sectors)?;

    burn_funds_and_notify_pledge_change(rt, &penalty)?;

    Ok(())
}

/// Removes a group sectors from the sector set and its number from all sector collections in state.
fn remove_terminated_sectors<BS>(
    st: &mut State,
    store: &BS,
    deadlines: &mut Deadlines,
    sectors: &BitField,
) -> Result<(), String>
where
    BS: BlockStore,
{
    st.delete_sector(store, sectors)?;
    st.remove_new_sectors(sectors);
    deadlines.remove_from_all_deadlines(sectors)?;
    st.remove_faults(store, sectors)?;
    st.remove_recoveries(sectors)?;
    Ok(())
}

fn enroll_cron_event<BS, RT>(
    rt: &mut RT,
    event_epoch: ChainEpoch,
    cb: CronEventPayload,
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let payload = Serialized::serialize(cb).map_err(|e| {
        ActorError::new(
            ExitCode::ErrIllegalArgument,
            format!("failed to serialize payload: {}", e),
        )
    })?;

    let ser_params = Serialized::serialize(EnrollCronEventParams {
        event_epoch,
        payload,
    })?;
    rt.send(
        &*STORAGE_POWER_ACTOR_ADDR,
        PowerMethod::EnrollCronEvent as u64,
        &ser_params,
        &TokenAmount::zero(),
    )?;

    Ok(())
}

fn request_begin_faults<BS, RT>(
    rt: &mut RT,
    sector_size: SectorSize,
    sectors: &[SectorOnChainInfo],
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if sectors.is_empty() {
        return Ok(());
    }

    let weights = sectors
        .iter()
        .map(|s| to_storage_weight_desc(sector_size, s))
        .collect();
    let ser_params = Serialized::serialize(OnFaultBeginParams { weights })?;

    rt.send(
        &*STORAGE_POWER_ACTOR_ADDR,
        PowerMethod::OnFaultBegin as u64,
        &ser_params,
        &TokenAmount::zero(),
    )?;
    Ok(())
}

fn request_end_faults<BS, RT>(
    rt: &mut RT,
    sector_size: SectorSize,
    sectors: &[SectorOnChainInfo],
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if sectors.is_empty() {
        return Ok(());
    }

    let weights = sectors
        .iter()
        .map(|s| to_storage_weight_desc(sector_size, s))
        .collect();
    let ser_params = Serialized::serialize(OnFaultEndParams { weights })?;

    rt.send(
        &*STORAGE_POWER_ACTOR_ADDR,
        PowerMethod::OnFaultEnd as u64,
        &ser_params,
        &TokenAmount::zero(),
    )?;
    Ok(())
}

fn request_terminate_deals<BS, RT>(rt: &mut RT, deal_ids: Vec<DealID>) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if deal_ids.is_empty() {
        return Ok(());
    }

    rt.send(
        &*STORAGE_MARKET_ACTOR_ADDR,
        MarketMethod::OnMinerSectorsTerminate as u64,
        &Serialized::serialize(OnMinerSectorsTerminateParams { deal_ids })?,
        &TokenAmount::zero(),
    )?;
    Ok(())
}
fn request_terminate_power<BS, RT>(
    rt: &mut RT,
    termination_type: SectorTermination,
    sector_size: SectorSize,
    sectors: &[SectorOnChainInfo],
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if sectors.is_empty() {
        return Ok(());
    }

    let weights = sectors
        .iter()
        .map(|s| to_storage_weight_desc(sector_size, s))
        .collect();
    let ser_params = Serialized::serialize(OnSectorTerminateParams {
        termination_type,
        weights,
    })?;

    rt.send(
        &*STORAGE_POWER_ACTOR_ADDR,
        PowerMethod::OnSectorTerminate as u64,
        &ser_params,
        &TokenAmount::zero(),
    )?;
    Ok(())
}

fn verify_windowed_post<BS, RT>(
    rt: &RT,
    challenge_epoch: ChainEpoch,
    sectors: &[SectorOnChainInfo],
    proofs: Vec<PoStProof>,
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let miner_actor_id: u64 = if let Payload::ID(i) = rt.message().receiver().payload() {
        *i
    } else {
        panic!("could not provide ID address");
    };

    // Regenerate challenge randomness, which must match that generated for the proof.
    let entropy = rt.message().receiver().marshal_cbor().unwrap();
    let randomness: PoStRandomness =
        rt.get_randomness(WindowedPoStChallengeSeed, challenge_epoch, &entropy)?;

    let challenged_sectors = sectors.iter().map(|s| s.to_sector_info()).collect();

    // get public inputs
    let pv_info = WindowPoStVerifyInfo {
        randomness,
        proofs,
        challenged_sectors,
        prover: miner_actor_id,
    };

    // verify the post proof
    rt.syscalls().verify_post(&pv_info).map_err(|e| {
        ActorError::new(
            ExitCode::ErrIllegalArgument,
            format!("invalid PoSt: {:?}, {}", pv_info, e),
        )
    })?;
    Ok(())
}
fn get_verify_info<BS, RT>(
    rt: &mut RT,
    params: SealVerifyParams,
) -> Result<SealVerifyInfo, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if rt.curr_epoch() <= params.interactive_epoch {
        return Err(ActorError::new(
            ExitCode::ErrForbidden,
            "too early to prove sector".to_string(),
        ));
    }

    // check randomness
    let challenge_earliest = seal_challenge_earliest(rt.curr_epoch(), params.registered_proof);
    if params.seal_rand_epoch < challenge_earliest {
        return Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            format!(
                "seal epoch {} too old, expected >= {}",
                params.seal_rand_epoch, challenge_earliest
            ),
        ));
    }

    let commd = request_unsealed_sector_cid(rt, params.registered_proof, params.deal_ids.clone())?;

    let miner_actor_id: u64 = if let Payload::ID(i) = rt.message().receiver().payload() {
        *i
    } else {
        panic!("could not provide ID address");
    };
    let entropy = rt.message().receiver().marshal_cbor().unwrap();
    let randomness: SealRandom =
        rt.get_randomness(SealRandomness, params.seal_rand_epoch, &entropy)?;
    let interactive_randomness: InteractiveSealRandomness = rt.get_randomness(
        InteractiveSealChallengeSeed,
        params.interactive_epoch,
        &entropy,
    )?;

    Ok(SealVerifyInfo {
        registered_proof: params.registered_proof,
        sector_id: SectorID {
            miner: miner_actor_id,
            number: params.sector_num,
        },
        deal_ids: params.deal_ids,
        interactive_randomness,
        proof: params.proof,
        randomness,
        sealed_cid: params.sealed_cid,
        unsealed_cid: commd,
    })
}
/// Requests the storage market actor compute the unsealed sector CID from a sector's deals.
fn request_unsealed_sector_cid<BS, RT>(
    rt: &mut RT,
    sector_type: RegisteredSealProof,
    deal_ids: Vec<DealID>,
) -> Result<Cid, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let ret = rt.send(
        &*STORAGE_MARKET_ACTOR_ADDR,
        MarketMethod::ComputeDataCommitment as u64,
        &Serialized::serialize(ComputeDataCommitmentParams {
            sector_type,
            deal_ids,
        })?,
        &TokenAmount::zero(),
    )?;
    let unsealed_cid: Cid = ret.deserialize()?;
    Ok(unsealed_cid)
}
fn commit_worker_key_change<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    rt.transaction(|st: &mut State, rt| {
        if st.info.pending_worker_key.is_none() {
            return Err(ActorError::new(
                ExitCode::ErrIllegalState,
                "No pending key change.".to_string(),
            ));
        }
        if let Some(worker_key) = &st.info.pending_worker_key {
            if worker_key.effective_at > rt.curr_epoch() {
                return Err(ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!(
                        "Too early for key change. Current: {}, Change: {}",
                        rt.curr_epoch(),
                        worker_key.effective_at
                    ),
                ));
            }

            st.info.worker = worker_key.new_worker;
            st.info.pending_worker_key = None;
        }

        Ok(())
    })?
}

/// Verifies that the total locked balance exceeds the sum of sector initial pledges.
fn verify_pledge_meets_initial_requirements<BS, RT>(_rt: &RT, _st: &State)
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    // TODO WPOST (follow-up): implement this
    todo!()
}

/// Resolves an address to an ID address and verifies that it is address of an account or multisig actor.
fn resolve_owner_address<BS, RT>(rt: &RT, raw: Address) -> Result<Address, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let resolved = rt.resolve_address(&raw).map_err(|_| {
        ActorError::new(
            ExitCode::ErrIllegalArgument,
            format!("unable to resolve address {}", raw),
        )
    })?;
    assert!(resolved.protocol() == Protocol::ID);

    let owner_code = rt.get_actor_code_cid(&resolved).map_err(|_| {
        ActorError::new(
            ExitCode::ErrIllegalArgument,
            format!("no code for address: {}", resolved),
        )
    })?;
    if !is_principal(&owner_code) {
        return Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            format!("owner actor type must be a principal, was {}", owner_code),
        ));
    }

    Ok(resolved)
}

/// Resolves an address to an ID address and verifies that it is address of an account actor with an associated BLS key.
/// The worker must be BLS since the worker key will be used alongside a BLS-VRF.
fn resolve_worker_address<BS, RT>(rt: &mut RT, raw: Address) -> Result<Address, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let resolved = rt.resolve_address(&raw).map_err(|e| {
        ActorError::new(
            ExitCode::ErrIllegalArgument,
            format!("unable to resolve address: {},{}", raw, e),
        )
    })?;
    assert!(resolved.protocol() == Protocol::ID);

    let owner_code = rt.get_actor_code_cid(&resolved).map_err(|e| {
        ActorError::new(
            ExitCode::ErrIllegalArgument,
            format!("no code for address: {}, {}", resolved, e),
        )
    })?;
    if owner_code != *ACCOUNT_ACTOR_CODE_ID {
        return Err(ActorError::new(
            ExitCode::ErrIllegalArgument,
            format!("worker actor type must be an account, was {}", owner_code),
        ));
    }

    if raw.protocol() != Protocol::BLS {
        let ret = rt.send(
            &resolved,
            AccountMethod::PubkeyAddress as u64,
            &Serialized::default(),
            &TokenAmount::zero(),
        )?;
        let pub_key: Address = ret.deserialize().map_err(|e| {
            ActorError::new(
                ExitCode::ErrSerialization,
                format!("failed to deserialize address result: {:?}, {}", ret, e),
            )
        })?;
        if pub_key.protocol() != Protocol::BLS {
            return Err(ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "worker account {} must have BLS pubkey, was {}",
                    resolved,
                    pub_key.protocol()
                ),
            ));
        }
    }
    Ok(resolved)
}

fn burn_funds_and_notify_pledge_change<BS, RT>(
    rt: &mut RT,
    amount: &TokenAmount,
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    burn_funds(rt, amount)?;
    notify_pledge_change(rt, &amount.clone().neg())
}

fn burn_funds<BS, RT>(rt: &mut RT, amount: &TokenAmount) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if amount > &BigInt::zero() {
        rt.send(
            &*BURNT_FUNDS_ACTOR_ADDR,
            METHOD_SEND,
            &Serialized::default(),
            amount,
        )?;
    }
    Ok(())
}
fn notify_pledge_change<BS, RT>(rt: &mut RT, pledge_delta: &BigInt) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if !pledge_delta.is_zero() {
        rt.send(
            &*STORAGE_POWER_ACTOR_ADDR,
            PowerMethod::UpdatePledgeTotal as u64,
            &Serialized::serialize(BigIntSer(pledge_delta))?,
            &TokenAmount::zero(),
        )?;
    }
    Ok(())
}
/// Assigns proving period offset randomly in the range [0, WPoStProvingPeriod) by hashing
/// the actor's address and current epoch.
fn assign_proving_period_offset(
    addr: Address,
    current_epoch: ChainEpoch,
    blake2b: impl FnOnce(&[u8]) -> Result<[u8; 32], Box<dyn StdError>>,
) -> Result<ChainEpoch, Box<dyn StdError>> {
    let mut my_addr = addr.marshal_cbor()?;
    BigEndian::write_i64(&mut my_addr, current_epoch);

    let digest = blake2b(&my_addr)?;

    let mut offset: ChainEpoch = BigEndian::read_i64(&digest);
    offset %= WPOST_PROVING_PERIOD;

    Ok(offset)
}

/// Computes the epoch at which a proving period should start such that it is greater than the current epoch, and
/// has a defined offset from being an exact multiple of WPoStProvingPeriod.
/// A miner is exempt from Winow PoSt until the first full proving period starts.
fn next_proving_period_start(current_epoch: ChainEpoch, offset: ChainEpoch) -> ChainEpoch {
    let curr_modulus = current_epoch % WPOST_PROVING_PERIOD;

    let period_progress = if curr_modulus >= offset {
        curr_modulus - offset
    } else {
        WPOST_PROVING_PERIOD - (offset - curr_modulus)
    };

    let period_start = current_epoch - period_progress + WPOST_PROVING_PERIOD;
    assert!(period_start > current_epoch);
    period_start
}

/// Computes deadline information for a fault or recovery declaration.
/// If the deadline has not yet elapsed, the declaration is taken as being for the current proving period.
/// If the deadline has elapsed, it's instead taken as being for the next proving period after the current epoch.
fn declaration_deadline_info(
    period_start: ChainEpoch,
    deadline_idx: usize,
    current_epoch: ChainEpoch,
) -> Result<DeadlineInfo, String> {
    if deadline_idx >= WPOST_PERIOD_DEADLINES {
        return Err(format!(
            "invalid deadline {}, must be < {}",
            deadline_idx, WPOST_PERIOD_DEADLINES
        ));
    }

    let mut deadline = DeadlineInfo::new(period_start, deadline_idx, current_epoch);
    // While deadline is in the past, roll over to the next proving period..
    if deadline.has_elapsed() {
        deadline = DeadlineInfo::new(deadline.next_period_start(), deadline_idx, current_epoch);
    }
    Ok(deadline)
}

/// Checks that a fault or recovery declaration of sectors at a specific deadline is valid and not within
/// the exclusion window for the deadline.
fn validate_fr_declaration(
    deadlines: &mut Deadlines,
    deadline: &DeadlineInfo,
    declared_sectors: &BitField,
) -> Result<(), String> {
    if deadline.fault_cutoff_passed() {
        return Err("late fault or recovery declaration".to_string());
    }

    // check that the declared sectors are actually due at the deadline
    let deadline_sectors = deadlines
        .due
        .get(deadline.index)
        .ok_or("deadline not found")?;
    if !deadline_sectors.contains_all(&declared_sectors) {
        return Err(format!(
            "sectors not all due at deadline {}",
            deadline.index
        ));
    }
    Ok(())
}

/// Computes a fee for a collection of sectors and unlocks it from unvested funds (for burning).
/// The fee computation is a parameter.
fn unlock_penalty<BS>(
    st: &mut State,
    store: &BS,
    current_epoch: ChainEpoch,
    sectors: &[SectorOnChainInfo],
    f: &impl Fn(&SectorOnChainInfo) -> TokenAmount,
) -> Result<TokenAmount, String>
where
    BS: BlockStore,
{
    let mut fee = BigInt::zero();
    for s in sectors {
        fee += f(s)
    }

    st.unlock_unvested_funds(store, current_epoch, fee)
}

/// The oldest seal challenge epoch that will be accepted in the current epoch.
fn seal_challenge_earliest(current_epoch: ChainEpoch, proof: RegisteredSealProof) -> ChainEpoch {
    current_epoch - CHAIN_FINALITYISH - max_seal_duration(proof).unwrap_or_default()
}

impl ActorCode for Actor {
    fn invoke_method<BS, RT>(
        &self,
        rt: &mut RT,
        method: MethodNum,
        params: &Serialized,
    ) -> Result<Serialized, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match FromPrimitive::from_u64(method) {
            Some(Method::Constructor) => {
                check_empty_params(params)?;
                Self::constructor(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::ControlAddresses) => {
                check_empty_params(params)?;
                let res = Self::control_addresses(rt)?;
                Ok(Serialized::serialize(&res)?)
            }
            Some(Method::ChangeWorkerAddress) => {
                Self::change_worker_address(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::ChangePeerID) => {
                Self::change_peer_ids(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::SubmitWindowedPoSt) => {
                Self::submit_windowed_post(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::PreCommitSector) => {
                Self::pre_commit_sector(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::ProveCommitSector) => {
                Self::prove_commit_sector(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::ExtendSectorExpiration) => {
                Self::extend_sector_expiration(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::TerminateSectors) => {
                Self::terminate_sectors(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::DeclareFaults) => {
                Self::declare_faults(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::DeclareFaultsRecovered) => {
                Self::declare_faults_recovered(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::OnDeferredCronEvent) => {
                Self::on_deferred_cron_event(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::CheckSectorProven) => {
                Self::check_sector_proven(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::AddLockedFund) => {
                let BigIntDe(param) = params.deserialize()?;
                Self::add_locked_fund(rt, param)?;
                Ok(Serialized::default())
            }
            Some(Method::ReportConsensusFault) => {
                Self::report_consensus_fault(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::WithdrawBalance) => {
                Self::withdraw_balance(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::ConfirmSectorProofsValid) => {
                Self::confirm_sector_proofs_valid(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::ChangeMultiaddrs) => {
                Self::change_multi_address(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            None => {
                // Method number does not match available, abort in runtime
                Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method".to_owned()))
            }
        }
    }
}
