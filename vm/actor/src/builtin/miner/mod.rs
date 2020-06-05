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
use bitfield::BitField;
use cid::{multihash::Blake2b256, Cid};
use clock::ChainEpoch;
use crypto::DomainSeparationTag::{
    InteractiveSealChallengeSeed, SealRandomness, WindowPoStDeadlineAssignment,
    WindowedPoStChallengeSeed,
};
use fil_types::{
    InteractiveSealRandomness, PoStProof, PoStRandomness, RegisteredProof,
    SealRandomness as SealRandom, SealVerifyInfo, SealVerifyParams, SectorID, SectorInfo,
    SectorNumber, SectorSize, WindowPoStVerifyInfo,
};
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use message::Message;
use num_bigint::biguint_ser::{BigUintDe, BigUintSer};
use num_bigint::BigUint;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Zero};
use runtime::{ActorCode, Runtime, Syscalls};
use std::collections::HashMap;
use vm::{
    ActorError, DealID, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR,
    METHOD_SEND,
};

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
}

/// Storage miner actors are created exclusively by the storage power actor. In order to break a circular dependency
/// between the two, the construction parameters are defined in the power actor.
type ConstructorParams = MinerConstructorParams;

/// Miner Actor
pub struct Actor;

/////////////////
// Constructor //
/////////////////
impl Actor {
    pub fn constructor<BS, RT>(rt: &mut RT, params: ConstructorParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*INIT_ACTOR_ADDR))?;

        if !check_supported_proof_types(params.seal_proof_type) {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "proof type {:?} not allowed for new miner actors",
                    params.seal_proof_type
                ),
            );
        };

        let owner = resolve_owner_address(rt, params.owner)?;
        let worker = resolve_worker_address(rt, params.worker)?;

        let empty_map = make_map(rt.store()).flush().map_err(|err| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("Failed to construct miner state: {}", err),
            )
        })?;

        let empty_root = Amt::<Cid, BS>::new(rt.store()).flush().map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("Failed to construct miner state: {}", e),
            )
        })?;

        let empty_deadlines = Deadlines::new();
        let empty_deadlines_cid = rt.store().put(&empty_deadlines, Blake2b256).unwrap();

        let current_epoch = rt.curr_epoch();
        let offset = assign_proving_period_offset(*rt.message().to(), current_epoch, rt.syscalls())
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrSerialization,
                    format!("failed to assign proving period offset: {}", e),
                )
            })?;

        let period_start = next_proving_period_start(current_epoch, offset).unwrap();
        assert!(period_start > current_epoch);

        let st = State::new(
            empty_root,
            empty_map,
            empty_deadlines_cid,
            owner,
            worker,
            params.peer_id,
            params.seal_proof_type,
        );
        rt.create(&st)?;

        // Register cron callback for epoch before the first proving period starts.
        enroll_cron_event(
            rt,
            period_start - 1,
            CronEventPayload {
                event_type: CRON_EVENT_PROVING_PERIOD,
                sectors: BitField::default(),
            },
        );

        Ok(())
    }

    /////////////
    // Control //
    /////////////
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
        let current_epoch = rt.curr_epoch();
        let mut effective_epoch = ChainEpoch::default();
        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            rt.validate_immediate_caller_is(std::iter::once(&st.info.owner))?;
            let worker = resolve_worker_address(rt, params.new_worker)?;

            effective_epoch = current_epoch + WORKER_KEY_CHANGE_DELAY;

            // This may replace another pending key change.
            st.info.pending_worker_key = Some(WorkerKeyChange {
                new_worker: worker,
                effective_at: effective_epoch,
            });
            Ok(())
        })?;

        let cron_payload = CronEventPayload {
            event_type: CRON_EVENT_WORKER_KEY_CHANGE,
            sectors: BitField::default(),
        };
        enroll_cron_event(rt, effective_epoch, cron_payload);
        Ok(())
    }

    fn change_peer_ids<BS, RT>(rt: &mut RT, params: ChangePeerIDParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.transaction::<State, Result<(), ActorError>, _>(|st: &mut State, rt| {
            rt.validate_immediate_caller_is(std::iter::once(&st.info.worker))?;
            st.info.peer_id = params.new_id;
            Ok(())
        })?;
        Ok(())
    }

    //////////////////
    // WindowedPoSt //
    //////////////////

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
        let mut sec_size = SectorSize::_2KiB;
        let mut detected_faults_sector: Vec<SectorOnChainInfo> = Vec::new();
        let mut recovered_sectors: Vec<SectorOnChainInfo> = Vec::new();
        let mut penalty = TokenAmount::default();

        rt.transaction::<State, Result<(), ActorError>, _>(|st: &mut State, rt| {
            rt.validate_immediate_caller_is(std::iter::once(&st.info.worker))?;

            sec_size = st.info.sector_size;
            let partitions_size = st.info.window_post_partition_sectors;
            let submission_partition_limit = window_post_message_partitions_max(partitions_size);
            if params.partitions.len() as u64 > submission_partition_limit {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!(
                        "too many partitions {}, limit {}",
                        params.partitions.len(),
                        submission_partition_limit
                    ),
                );
            }
            let deadline = match st.deadline_info(current_epoch) {
                Some(deadline) => deadline,
                None => {
                    return Err(ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        format!("no deadline info found for epoch {}", current_epoch),
                    ))
                }
            };

            if !deadline.period_start() {
                return Err(ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!(
                        "proving period {} not yet open at {}",
                        deadline.period_start(),
                        current_epoch
                    ),
                ));
            }
            if deadline.period_elapsed() {
                // A cron event has not yet processed the previous proving period and established the next one.
                // This is possible in the first non-empty epoch of a proving period if there was an empty tipset on the
                // last epoch of the previous period.
                return Err(ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!(
                        "proving period at {} elapsed, next one not yet opened",
                        deadline.period_start()
                    ),
                ));
            }
            if params.deadline != deadline.index {
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

            let mut deadlines = st.load_deadlines(rt.store()).map_err(|_| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to load deadlines"),
                )
            })?;

            // Traverse earlier submissions and enact detected faults.
            // This isn't strictly necessary, but keeps the power table up to date eagerly and can force payment
            // of penalties if locked pledge drops too low.
            let (detected_faults, p) = process_missing_post_faults(
                rt,
                st,
                rt.store(),
                &mut deadlines,
                &deadline.period_start,
                deadline.index,
                &current_epoch,
            )?;
            detected_faults_sector = detected_faults;
            penalty = p;
            // TODO WPOST (follow-up): process Skipped as faults

            // Work out which sectors are due in the declared partitions at this deadline.
            let partitions_sectors = compute_partitions_sector(
                deadlines,
                partitions_size,
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

            let proven_sectors = BitField::union(&partitions_sectors).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to union partitions of sectors: {}", e),
                )
            })?;

            let (sector_infos, mut declared_recoveries) = st
                .load_sector_infos_for_proof(rt.store(), proven_sectors)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to load proven sector info: {}", e),
                    )
                })?;

            // Verify the proof.
            // A failed verification doesn't immediately cause a penalty; the miner can try again.
            verify_windowed_post(rt, deadline.challenge, &sector_infos, &params.proofs);

            // Record the successful submission
            let mut posted_partitions = BitField::new_from_set(&params.partitions);
            let contains = st
                .post_submissions
                .contains_any(&mut posted_partitions)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to intersect post partitions: {}", e),
                    )
                })?;
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
            st.remove_faults(rt.store(), &mut declared_recoveries)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to remove recoveries from faults: {}", e),
                    )
                })?;

            st.remove_recoveries(&mut declared_recoveries)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to remove recoveries: {}", e),
                    )
                })?;

            // Load info for recovered sectors for recovery of power outside this state transaction.
            let empty = declared_recoveries.is_empty().map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to check if bitfield was empty: {}", e),
                )
            })?;

            if !empty {
                let mut sectors_by_number: HashMap<SectorNumber, SectorOnChainInfo> =
                    HashMap::new();
                for s in sector_infos {
                    sectors_by_number.insert(s.info.sector_number, s);
                }
                declared_recoveries.for_each(|i| {
                    let key: SectorNumber = i as u64;
                    let s = sectors_by_number.get(&key).cloned().unwrap();
                    recovered_sectors.push(s);
                    Ok(())
                });
            }
            Ok(())
        })?;
        // Remove power for new faults, and burn penalties.
        request_begin_faults(rt, sec_size, detected_faults_sector);
        burn_funds_and_notify_pledge_change(rt, &penalty);

        // restore power for recovered sectors
        if recovered_sectors.len() > 0 {
            request_end_faults(rt, sec_size, recovered_sectors);
        }
        Ok(())
    }

    ///////////////////////
    // Sector Commitment //
    ///////////////////////

    /// Proposals must be posted on chain via sma.PublishStorageDeals before PreCommitSector.
    /// Optimization: PreCommitSector could contain a list of deals that are not published yet.
    fn pre_commit_sector<BS, RT>(rt: &mut RT, params: SectorPreCommitInfo) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if params.expiration <= rt.curr_epoch() {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "sector expiration {} must be after now {}",
                    params.expiration,
                    rt.curr_epoch()
                ),
            );
        }
        if params.seal_rand_epoch >= rt.curr_epoch() {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "seal challenge epoch {} must be before now {}",
                    params.seal_rand_epoch,
                    rt.curr_epoch()
                ),
            );
        }
        let challenge_earliest = seal_challenge_earliest(rt.curr_epoch(), params.registered_proof);
        if params.seal_rand_epoch < challenge_earliest {
            // The subsequent commitment proof can't possibly be accepted because the seal challenge will be deemed
            // too old. Note that passing this check doesn't guarantee the proof will be soon enough, depending on
            // when it arrives.
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "seal challenge epoch {} too old, must be after {}",
                    params.seal_rand_epoch, challenge_earliest
                ),
            );
        }

        let newly_vested_amount = rt.transaction::<State, Result<TokenAmount, ActorError>, _>(|st, rt| {
            rt.validate_immediate_caller_is(std::iter::once(&st.info.worker))?;
            if params.registered_proof != st.info.seal_proof_type {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("wrong proof type {:?}", params.registered_proof),
                );
            };
            st.get_precommitted_sector(rt.store(), params.sector_number).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!(
                        "failed to check precommitted sector: {}, {}",
                        params.sector_number, e
                    ),
                )
            })?
            .ok_or_else(|| {
                ActorError::new(
                    ExitCode::ErrNotFound,
                    format!("no precommitted sector: {}", params.sector_number),
                )
            })?;

            let sector_info = st.get_sector(rt.store(), params.sector_number).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to check sector: {}, {}", params.sector_number, e),
                )
                })?.ok_or_else(|| {
                    ActorError::new(
                        ExitCode::ErrNotFound,
                        format!("no precommitted sector: {}", params.sector_number),
                    )
                })?;
                if sector_info.info.deal_ids.len() > 0 {
                    // Sector has been previously committed and proven with deals.
                    ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        format!("sector already committed with deals: {}", params.sector_number)
                    );
                } else {
                    // Committed Capacity sector upgrade.
                    if params.expiration < sector_info.info.expiration {
                        ActorError::new(
                            ExitCode::ErrIllegalArgument,
                            format!("upgraded sector {} expires before original expiration", params.sector_number)
                        );
                    }
                }

            // Check expiry is exactly *the epoch before* the start of a proving period.
            let period_offset = st.proving_period_start % WPOST_PROVING_PERIOD;
            let expiry_offset = (params.expiration + 1) % WPOST_PROVING_PERIOD;
            if expiry_offset != period_offset {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("invalid expiration {}, must be immediately before proving period boundary {} mod {}", params.expiration, period_offset, WPOST_PROVING_PERIOD),
                );
            }
            let newly_vested_amount = st.unlock_vested_funds(rt.store(), rt.curr_epoch()).unwrap();
            let available_balance = st.get_available_balance(&rt.current_balance()?);
            let deposit_req = precommit_deposit(*st.get_sector_size(), &(params.expiration - rt.curr_epoch()));
            if available_balance < deposit_req {
                ActorError::new(
                    ExitCode::ErrInsufficientFunds,
                    format!("insufficient funds for pre-commit deposit: {}", deposit_req),
                );
            }
            st.add_pre_commit_deposit(&deposit_req);
            st.assert_balance_invariants(&rt.current_balance()?);
            st.put_precommitted_sector(rt.store(), SectorPreCommitOnChainInfo{
                info: params.clone(),
                pre_commit_deposit: deposit_req,
                pre_commit_epoch: rt.curr_epoch()
            }).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to write pre-committed sector: {}, {}", params.sector_number, e),
                )
            })?;
            Ok(newly_vested_amount)
        })??;

        notify_pledge_change(rt, &newly_vested_amount);
        let mut bf = BitField::new();
        bf.set(params.sector_number);

        // Request deferred Cron check for PreCommit expiry check.
        let cron_payload = CronEventPayload {
            event_type: CRON_EVENT_PRE_COMMIT_EXPIRY,
            sectors: bf,
        };

        let msd = max_seal_duration(&params.registered_proof).ok_or_else(|| {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "no max seal duration set for proof type: {:?}",
                    params.registered_proof
                ),
            )
        })?;
        let expiry_bound = rt.curr_epoch() + msd + 1;
        enroll_cron_event(rt, expiry_bound, cron_payload);

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

        let msd = max_seal_duration(&precommit.info.registered_proof).ok_or_else(|| {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "no max seal duration set for proof type: {:?}",
                    precommit.info.registered_proof
                ),
            )
        })?;
        let prove_commit_due = precommit.pre_commit_epoch + msd;
        if rt.curr_epoch() > prove_commit_due {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "commitment proof for {} too late at {}, due {}",
                    sector_number,
                    rt.curr_epoch(),
                    prove_commit_due
                ),
            );
        }

        // will abort if seal invalidgetVerifyInfo
        get_verify_info(
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
        );

        // Check (and activate) storage deals associated to sector. Abort if checks failed.
        // return DealWeight for the deal set in the sector
        let mut method_num = MarketMethod::VerifyDealsOnSectorProveCommit as u64; // todo ask
        let mut ser_params = Serialized::serialize(&VerifyDealsOnSectorProveCommitParams {
            deal_ids: precommit.info.deal_ids.clone(),
            sector_expiry: precommit.info.expiration,
        })?;
        let mut ret = rt.send(
            &*STORAGE_MARKET_ACTOR_ADDR,
            method_num,
            &ser_params,
            &BigUint::zero(),
        )?;
        let deal_weights: VerifyDealsOnSectorProveCommitReturn = ret.deserialize()?;

        // Request power for activated sector.
        // Return initial pledge requirement.
        method_num = PowerMethod::OnSectorProveCommit as u64;
        ser_params = Serialized::serialize(OnSectorProveCommitParams {
            weight: SectorStorageWeightDesc {
                sector_size: st.info.sector_size,
                deal_weight: deal_weights.deal_weight.clone(),
                verified_deal_weight: deal_weights.verified_deal_weight.clone(),
                duration: precommit.info.expiration - rt.curr_epoch(),
            },
        })?;
        ret = rt.send(
            &*STORAGE_MARKET_ACTOR_ADDR,
            method_num,
            &ser_params,
            &BigUint::zero(),
        )?;
        let BigUintDe(initial_pledge) = ret.deserialize()?;

        // Add sector and pledge lock-up to miner state
        let newly_vested_amount =
            rt.transaction::<State, Result<TokenAmount, ActorError>, _>(|st: &mut State, rt| {
                let vested_amount = st
                    .unlock_vested_funds(rt.store(), rt.curr_epoch())
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to vest new funds: {}", e),
                        )
                    })?;

                // Unlock deposit for successful proof, make it available for lock-up as initial pledge.
                st.add_pre_commit_deposit(&precommit.pre_commit_deposit);

                // Verify locked funds are are at least the sum of sector initial pledges.
                verify_pledge_meets_initial_requirements(rt, &st);

                // Lock up initial pledge for new sector.
                let available_balance = st.get_available_balance(&rt.current_balance()?);
                if available_balance > initial_pledge {
                    ActorError::new(
                        ExitCode::ErrInsufficientFunds,
                        format!(
                            "insufficient funds for initial pledge requirement {}, available: {}",
                            initial_pledge, available_balance
                        ),
                    );
                }
                st.add_locked_funds(
                    rt.store(),
                    &rt.curr_epoch(),
                    &initial_pledge,
                    PLEDGE_VESTING_SPEC,
                )
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to add pledge: {}", e),
                    )
                })?;

                st.assert_balance_invariants(&rt.current_balance()?);

                let new_sector_info = SectorOnChainInfo {
                    info: precommit.info.clone(),
                    activation_epoch: rt.curr_epoch(),
                    deal_weight: deal_weights.deal_weight,
                    verified_deal_weight: deal_weights.verified_deal_weight,
                };

                st.put_sector(rt.store(), new_sector_info).map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to prove commit: {}", e),
                    )
                })?;

                st.delete_precommitted_sector(rt.store(), sector_number)
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!(
                                "failed to delete precommit for sector: {}, {}",
                                sector_number, e
                            ),
                        )
                    })?;

                st.add_sector_expirations(rt.store(), precommit.info.expiration, &[sector_number])
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!(
                                "failed to add new sector {} expiration: {}",
                                sector_number, e
                            ),
                        )
                    })?;

                // Add to new sectors, a staging ground before scheduling to a deadline at end of proving period.
                st.add_new_sectors(&[sector_number]).map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to add new sector number {}: {}", sector_number, e),
                    )
                })?;

                Ok(vested_amount)
            })??;
        let delta = initial_pledge - newly_vested_amount;
        notify_pledge_change(rt, &delta);

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

        let sec = st
            .get_sector(rt.store(), params.sector_number)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrNotFound,
                    format!("Sector hasn't been proven: {}, {}", params.sector_number, e),
                )
            })?;
        if sec.is_none() {
            ActorError::new(
                ExitCode::ErrNotFound,
                format!("sector hasn't been proven {}", params.sector_number),
            );
        }

        Ok(())
    }

    /////////////////////////
    // Sector Modification //
    /////////////////////////

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

        let mut sector = match st
            .get_sector(rt.store(), params.sector_number)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to add load sector {}: {}", params.sector_number, e),
                )
            })? {
            Some(sector) => sector,
            None => {
                return Err(ActorError::new(
                    ExitCode::ErrNotFound,
                    format!("no such sector {}", params.sector_number),
                ))
            }
        };

        let storage_weight_desc_prev = as_storage_weight_desc(&st.info.sector_size, &sector);
        let extension_len = params.new_expiration - sector.info.expiration.clone();
        // todo ask about negative value

        let mut storage_weight_desc_new = storage_weight_desc_prev.clone();
        storage_weight_desc_new.duration = storage_weight_desc_prev.duration + extension_len;

        let method_num = PowerMethod::OnSectorModifyWeightDesc as u64;
        let ser_params = Serialized::serialize(OnSectorModifyWeightDescParams {
            prev_weight: storage_weight_desc_prev,
            new_weight: storage_weight_desc_new,
        })?;

        rt.send(
            &*STORAGE_POWER_ACTOR_ADDR,
            method_num,
            &ser_params,
            &BigUint::zero(),
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
            Ok(())
        })?
    }

    fn terminate_sectors<BS, RT>(
        rt: &mut RT,
        mut params: TerminateSectorsParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let st: State = rt.state()?;
        rt.validate_immediate_caller_is(std::iter::once(&st.info.worker))?;

        // Note: this cannot terminate pre-committed but un-proven sectors.
        // They must be allowed to expire (and deposit burnt).
        terminate_sectors(rt, &mut params.sectors, SECTOR_TERMINATION_MANUAL);
        Ok(())
    }

    ////////////
    // Faults //
    ////////////

    fn declare_faults<BS, RT>(rt: &mut RT, params: DeclareFaultsParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if params.faults.len() as u64 > WPOST_PERIOD_DEADLINES {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "too many declarations {}, max {}",
                    params.faults.len(),
                    WPOST_PERIOD_DEADLINES
                ),
            );
        }
        let current_epoch = rt.curr_epoch();
        let mut declared_fault_sectors: Vec<SectorOnChainInfo> = Vec::new();
        let mut detected_fault_sectors: Vec<SectorOnChainInfo> = Vec::new();
        let mut penalty = BigUint::zero();
        let state: State = rt.state()?;

        rt.transaction::<State, Result<(), ActorError>, _>(|st: &mut State, rt| {
            rt.validate_immediate_caller_is(std::iter::once(&st.info.worker))?;

            let current_deadline = match st.deadline_info(current_epoch) {
                Some(current_deadline) => current_deadline,
                None => {
                    return Err(ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        format!("no deadline info found for epoch {}", current_epoch),
                    ))
                }
            };
            let mut deadlines = st.load_deadlines(rt.store()).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to load deadlines: {}", e),
                )
            })?;

            // Traverse earlier submissions and enact detected faults.
            // This is necessary to prevent the miner "declaring" a fault for a PoSt already missed.
            let (detected_faults, fine) = process_missing_post_faults(
                rt,
                st,
                rt.store(),
                &mut deadlines,
                &current_deadline.period_start,
                current_deadline.index,
                &current_epoch,
            )?;
            detected_fault_sectors = detected_faults;
            penalty = fine;

            let mut declared_sectors: Vec<BitField> = Vec::new();
            for mut decl in params.faults {
                let target_deadline: DeadlineInfo = declaration_deadline_info(
                    st.proving_period_start,
                    decl.deadline,
                    current_epoch,
                )
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        format!("invalid fault declaration deadline: {}", e),
                    )
                })?;
                validate_fr_declaration(&mut deadlines, &target_deadline, &mut decl.sectors)
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalArgument,
                            format!("invalid fault declaration: {}", e),
                        )
                    })?;
                declared_sectors.push(decl.sectors);
            }

            let all_declared = BitField::union(&declared_sectors).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to union faults: {}", e),
                )
            })?;

            // Split declarations into declarations of new faults, and retraction of declared recoveries.
            let mut recoveries = st
                .recoveries
                .clone()
                .intersect(&all_declared)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to intersect sectors with recoveries: {}", e),
                    )
                })?;

            let mut new_faults = all_declared.subtract(&recoveries).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to subtract recoveries from sectors: {}", e),
                )
            })?;
            let empty = new_faults.is_empty().map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to check if bitfield was empty: {}", e),
                )
            })?;

            if !empty {
                // check new fault are really new
                let contains = st.faults.contains_any(&mut new_faults).map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to intersect existing faults: {}", e),
                    )
                })?;
                if contains {
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
                st.add_faults(rt.store(), &mut new_faults, st.proving_period_start)
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to add faults: {}", e),
                        )
                    })?;
            }

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
            let declared_fault_sectors = st
                .load_sector_infos(rt.store(), &mut new_faults)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to load fault sectors: {}", e),
                    )
                })?;

            // Unlock penalty for declared faults.
            let declared_penalty = unlock_penalty(
                st,
                rt.store(),
                &current_epoch,
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

            let empty = recoveries.is_empty().map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to check if bitfield was empty: {}", e),
                )
            })?;

            if !empty {
                st.remove_recoveries(&mut recoveries).map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to remove recoveries: {}", e),
                    )
                })?;
            }

            Ok(())
        })?;

        // remove power for new faulty sectors
        detected_fault_sectors.append(&mut declared_fault_sectors);
        request_begin_faults(rt, state.info.sector_size, detected_fault_sectors);
        burn_funds_and_notify_pledge_change(rt, &penalty);

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
        if params.recoveries.len() as u64 > WPOST_PERIOD_DEADLINES {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!(
                    "too many declarations {}, max {}",
                    params.recoveries.len(),
                    WPOST_PERIOD_DEADLINES
                ),
            );
        }
        let mut detected_fault_sectors: Vec<SectorOnChainInfo> = Vec::new();
        let mut penalty = TokenAmount::default();
        let current_epoch = rt.curr_epoch();
        let state: State = rt.state()?;

        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            rt.validate_immediate_caller_is(std::iter::once(&st.info.worker))?;

            let current_deadline = match st.deadline_info(current_epoch) {
                Some(current_deadline) => current_deadline,
                None => {
                    return Err(ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        format!("no deadline info found for epoch {}", current_epoch),
                    ))
                }
            };
            let mut deadlines = st.load_deadlines(rt.store()).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to load deadlines: {}", e),
                )
            })?;

            // Traverse earlier submissions and enact detected faults.
            // This is necessary to move the NextDeadlineToProcessFaults index past the deadline that this recovery
            // is targeting, so that the recovery won't be declared failed next time it's checked during this proving period.
            let (fault_sectors, fine) =
                detect_faults_this_period(rt, st, rt.store(), &current_deadline, &mut deadlines)?;
            detected_fault_sectors = fault_sectors;
            penalty = fine;

            let mut declared_sectors: Vec<BitField> = Vec::new();
            for mut decl in params.recoveries {
                // TODO handle optional epoch
                let target_deadline: DeadlineInfo = declaration_deadline_info(
                    st.proving_period_start,
                    decl.deadline,
                    current_epoch,
                )
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        format!("invalid recovery declaration deadline: {}", e),
                    )
                })?;

                validate_fr_declaration(&mut deadlines, &target_deadline, &mut decl.sectors)
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalArgument,
                            format!("invalid recovery declaration: {}", e),
                        )
                    })?;
                declared_sectors.push(decl.sectors);
            }

            let mut all_recoveries = BitField::union(&declared_sectors).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to union recoveries: {}", e),
                )
            })?;

            let mut contains = st.faults.contains_all(&mut all_recoveries).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to check recoveries are faulty: {}", e),
                )
            })?;
            if !contains {
                return Err(ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    "declared recoveries not currently faulty".to_string(),
                ));
            }
            contains = st
                .recoveries
                .contains_any(&mut all_recoveries)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to intersect new recoveries: {}", e),
                    )
                })?;
            if contains {
                return Err(ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    "sector already declared recovered".to_string(),
                ));
            }

            st.add_recoveries(&mut all_recoveries).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("invalid recoveries: {}", e),
                )
            })?;

            Ok(())
        })?;

        // remove power for new faulty sectors
        request_begin_faults(rt, state.info.sector_size, detected_fault_sectors);
        burn_funds_and_notify_pledge_change(rt, &penalty);

        Ok(())
    }

    ///////////////////////
    // Pledge Collateral //
    ///////////////////////

    /// Locks up some amount of a the miner's unlocked balance (including any received alongside the invoking message).
    fn add_locked_fund<BS, RT>(rt: &mut RT, amount: TokenAmount) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let vested_amount =
            rt.transaction::<State, Result<TokenAmount, ActorError>, _>(|st, rt| {
                rt.validate_immediate_caller_is(
                    [st.info.worker, st.info.owner, *REWARD_ACTOR_ADDR].iter(),
                )?;

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
                    ActorError::new(
                        ExitCode::ErrInsufficientFunds,
                        format!(
                            "insufficient funds to lock, available: {}, requested: {}",
                            available_balance, amount
                        ),
                    );
                }

                st.add_locked_funds(rt.store(), &rt.curr_epoch(), &amount, PLEDGE_VESTING_SPEC)
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to lock pledge: {}", e),
                        )
                    })?;
                Ok(newly_vested_amount)
            })??;
        let delta = amount - vested_amount;
        notify_pledge_change(rt, &delta);
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
        let st: State = rt.state()?;
        let reporter = rt.message().from().clone();

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

        let method_num = PowerMethod::OnConsensusFault as u64;
        let ser_params = Serialized::serialize(BigUintSer(&st.locked_funds))?;
        rt.send(
            &*STORAGE_POWER_ACTOR_ADDR,
            method_num,
            &ser_params,
            &BigUint::zero(),
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
        assert!(&amount_withdrawn < &curr_balance);

        rt.send(
            &st.info.owner,
            METHOD_SEND,
            &Serialized::default(),
            &amount_withdrawn,
        )?;

        notify_pledge_change(rt, &vested_amount);

        st.assert_balance_invariants(&rt.current_balance()?);
        Ok(())
    }

    //////////
    // Cron //
    //////////

    fn on_deferred_cron_event<BS, RT>(
        rt: &mut RT,
        mut payload: CronEventPayload,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match payload.event_type {
            CRON_EVENT_PROVING_PERIOD => handle_proving_period(rt)?,
            CRON_EVENT_PRE_COMMIT_EXPIRY => check_precommit_expiry(rt, &mut payload.sectors)?,
            CRON_EVENT_WORKER_KEY_CHANGE => commit_worker_key_change(rt)?,
            _ => (),
        };

        Ok(())
    }
}

////////////////////////////////////////////////////////////////////////////////
// Utility functions & helpers
////////////////////////////////////////////////////////////////////////////////

/// Invoked at the end of each proving period, at the end of the epoch before the next one starts.
fn handle_proving_period<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let st: State = rt.state()?;
    // Note: because the cron actor is not invoked on epochs with empty tipsets, the current epoch is not necessarily
    // exactly the final epoch of the period; it may be slightly later (i.e. in the subsequent period).
    // Further, this method is invoked once *before* the first proving period starts, after the actor is first
    // constructed; this is detected by !deadline.PeriodStarted().
    // Use deadline.PeriodEnd() rather than rt.CurrEpoch unless certain of the desired semantics.

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

    notify_pledge_change(rt, &vested_amount);

    // Detect and penalize missing proofs.
    let mut detected_fault_sectors: Vec<SectorOnChainInfo> = Vec::new();
    let curr_epoch = rt.curr_epoch();
    let mut penalty = TokenAmount::default();
    let mut deadline = DeadlineInfo::default();
    rt.transaction::<State, Result<(), ActorError>, _>(|st: &mut State, rt| {
        deadline = st.deadline_info(curr_epoch).ok_or_else(|| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!(
                    "failed to load deadline info for current epoch {:}",
                    curr_epoch
                ),
            )
        })?;
        if deadline.period_start() {
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
                &deadline.period_start,
                deadline.index,
                &curr_epoch,
            )?;
            detected_fault_sectors = detected_faults;
            penalty = p;
        }
        Ok(())
    })?;

    // Remove power for new faults, and burn penalties.
    request_begin_faults(rt, st.info.sector_size, detected_fault_sectors);
    burn_funds_and_notify_pledge_change(rt, &penalty);

    let mut expired_sectors: BitField = BitField::new();
    rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
        expired_sectors =
            pop_sector_expirations(st, rt.store(), deadline.period_end()).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to load expired sectors {:}", e),
                )
            })?;
        Ok(())
    })?;

    // Terminate expired sectors (sends messages to power and market actors).
    terminate_sectors(rt, &mut expired_sectors, SECTOR_TERMINATION_EXPIRED);

    // Terminate sectors with faults that are too old, and pay fees for ongoing faults.
    let mut expired_faults: BitField = BitField::new();
    let mut ongoing_faults: BitField = BitField::new();
    let mut ongoing_fault_penalty = TokenAmount::default();
    rt.transaction::<State, Result<(), ActorError>, _>(|st: &mut State, rt| {
        // handle err with actor err
        let (exp_faults, on_faults) =
            pop_expired_faults(st, rt.store(), deadline.period_end() - FAULT_MAX_AGE).map_err(
                |e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to load fault sectors: {}", e),
                    )
                },
            )?;
        expired_faults = exp_faults;
        ongoing_faults = on_faults;

        // Load info for ongoing faults.
        // TODO: this is potentially super expensive for a large miner with ongoing faults
        let ongoing_fault_info = st
            .load_sector_infos(rt.store(), &mut ongoing_faults)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to charge fault fee: {}", e),
                )
            })?;

        // Unlock penalty for ongoing faults.
        ongoing_fault_penalty = unlock_penalty(
            st,
            rt.store(),
            &deadline.period_end(),
            &ongoing_fault_info,
            &pledge_penalty_for_sector_declared_fault,
        )
        .map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("failed to charge fault fee: {}", e),
            )
        })?;
        Ok(())
    })?;

    terminate_sectors(rt, &mut expired_faults, SECTOR_TERMINATION_FAULTY);
    burn_funds_and_notify_pledge_change(rt, &ongoing_fault_penalty);

    rt.transaction::<State, Result<(), ActorError>, _>(|st: &mut State, rt| {
        let mut deadlines = st.load_deadlines(rt.store()).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("failed to load deadlines {:}", e),
            )
        })?;

        // assign new sectors to deadlines
        let new_sectors = st
            .new_sectors
            .all(NEW_SECTORS_PER_PERIOD_MAX)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to expand new sectors {:}", e),
                )
            })?;

        if new_sectors.len() > 0 {
            let assignment_seed =
                rt.get_randomness(WindowPoStDeadlineAssignment, deadline.period_end(), &[])?;
            assign_new_sectors(
                &mut deadlines,
                st.info.window_post_partition_sectors,
                &new_sectors,
                assignment_seed,
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
        if deadline.period_start() {
            st.proving_period_start += WPOST_PROVING_PERIOD;
        }
        Ok(())
    })?;

    // Schedule cron callback for next period
    let next_period_end = st.proving_period_start + WPOST_PROVING_PERIOD - 1;
    enroll_cron_event(
        rt,
        next_period_end,
        CronEventPayload {
            event_type: CRON_EVENT_PROVING_PERIOD,
            sectors: BitField::default(),
        },
    );
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
                curr_deadline.period_start()
            ),
        ));
    }

    Ok(process_missing_post_faults(
        rt,
        st,
        store,
        deadlines,
        &curr_deadline.period_start,
        curr_deadline.index,
        &curr_deadline.current_epoch,
    )?)
}

/// Detects faults from missing PoSt submissions that did not arrive by some deadline, and moves
/// the NextDeadlineToProcessFaults index up to that deadline.
fn process_missing_post_faults<BS, RT>(
    _rt: &RT,
    st: &mut State,
    store: &BS,
    deadlines: &mut Deadlines,
    period_start: &ChainEpoch,
    before_deadline: u64,
    current_epoch: &ChainEpoch,
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

    let (mut detected_faults, mut failed_recoveries) = compute_faults_from_missing_posts(
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

    st.add_faults(store, &mut detected_faults, *period_start)
        .map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("failed to record new faults: {}", e),
            )
        })?;

    st.remove_recoveries(&mut failed_recoveries).map_err(|e| {
        ActorError::new(
            ExitCode::ErrIllegalState,
            format!("failed to record failed recoveries: {}", e),
        )
    })?;

    // Load info for sectors.
    // TODO: this is potentially super expensive for a large miner failing to submit proofs.
    let mut detected_fault_sectors =
        st.load_sector_infos(store, &mut detected_faults)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to load fault sectors: {}", e),
                )
            })?;
    let mut failed_recovery_sectors = st
        .load_sector_infos(store, &mut failed_recoveries)
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
        &current_epoch,
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
    since_deadline: u64,
    before_deadline: u64,
) -> Result<(BitField, BitField), String> {
    // TODO: Iterating this bitfield and keeping track of what partitions we're expecting could remove the
    // need to expand this into a potentially-giant map. But it's tricksy.
    let partition_size = st.info.window_post_partition_sectors;
    let submissions = st
        .post_submissions
        .all_set(active_partitions_max(partition_size))?;

    let mut f_groups: Vec<BitField> = Vec::new();
    let mut r_groups: Vec<BitField> = Vec::new();
    let mut deadline_first_partition: u64 = 0;

    let mut dl_idx = since_deadline;
    while dl_idx < before_deadline {
        let (dl_part_count, dl_sector_count) = deadline_count(deadlines, partition_size, dl_idx)?;
        let deadline_sectors = deadlines
            .due
            .get_mut(dl_idx as usize)
            .ok_or("deadline not found")?;

        let mut dl_part_idx: u64 = 0;
        while dl_part_idx < dl_part_count {
            if !submissions.contains(&(deadline_first_partition + dl_part_idx)) {
                // no PoSt received in prior period
                let part_first_sector_idx = dl_part_idx * partition_size;
                let part_sector_count =
                    std::cmp::min(partition_size, dl_sector_count - part_first_sector_idx);

                let partition_sectors =
                    deadline_sectors.slice(part_first_sector_idx, part_sector_count)?;

                // record newly-faulty sectors
                let new_faults = st.faults.clone().subtract(&partition_sectors)?;
                f_groups.push(new_faults);

                // record failed recoveries
                let failed_recovery = st.recoveries.clone().intersect(&partition_sectors)?;
                r_groups.push(failed_recovery);
            }
            dl_part_idx += 1;
        }
        deadline_first_partition += dl_part_count;
        dl_idx += 1;
    }
    let detected_faults = BitField::union(&f_groups)?;
    let failed_recoveries = BitField::union(&r_groups)?;

    Ok((detected_faults, failed_recoveries))
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
    });

    st.clear_sector_expirations(store, &expired_epochs)?;

    let all_expiries = BitField::union(&expired_sectors)?;

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
            all_expiries.merge_assign(faults);
            expired_epochs.push(fault_start);
        } else {
            all_ongoing_faults.merge_assign(faults);
        }
        Ok(())
    });

    st.clear_fault_epochs(store, &expired_epochs)?;

    let all_expiries = BitField::union(&[all_expiries])?;
    let all_ongoing_faults = BitField::union(&[all_ongoing_faults])?;

    Ok((all_expiries, all_ongoing_faults))
}

fn check_precommit_expiry<BS, RT>(rt: &mut RT, sectors: &mut BitField) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    // initialize here to add together for all sectors and minimize calls across actors
    let mut deposit_burn = TokenAmount::default();
    rt.transaction::<State, Result<(), ActorError>, _>(|st: &mut State, rt| {
        sectors
            .for_each(|i| {
                let sec_num: SectorNumber = i;
                let sector = st
                    .get_precommitted_sector(rt.store(), sec_num)?
                    .ok_or_else(|| format!("no precommitted sector: {}", sec_num))?;

                // delete actor
                st.delete_precommitted_sector(rt.store(), sec_num)?;

                // increment deposit to burn
                deposit_burn += sector.pre_commit_deposit;

                Ok(())
            })
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to check precommit expires: {}", e),
                )
            })?;

        st.pre_commit_deposit -= &deposit_burn;

        Ok(())
    })?;
    // This deposit was locked separately to pledge collateral so there's no pledge change here.
    burn_funds(rt, &deposit_burn);
    Ok(())
}

// TODO: red flag that this method is potentially super expensive
fn terminate_sectors<BS, RT>(
    rt: &mut RT,
    sector_nos: &mut BitField,
    termination_type: SectorTermination,
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let empty = sector_nos.is_empty().map_err(|_| {
        ActorError::new(
            ExitCode::ErrIllegalState,
            "failed to count sectors".to_string(),
        )
    })?;

    if empty {
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
        let mut faults = st.faults.clone().intersect(&sector_nos).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("failed to load faults: {}", e),
            )
        })?;

        let faults_map = faults.all_set(max_allowed_faults as usize).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("failed to expand faults: {}", e),
            )
        })?;

        sector_nos
            .for_each(|i| {
                let sector = st
                    .get_sector(rt.store(), i)?
                    .ok_or_else(|| format!("no sector: {}", i))?;

                deal_ids.extend(&sector.info.deal_ids);
                let fault = faults_map.contains(&i);
                if fault {
                    faulty_sectors.push(sector.clone());
                }

                all_sectors.push(sector);
                Ok(())
            })
            .map_err(|e| {
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
            // unhandled err
            penalty = unlock_penalty(
                st,
                rt.store(),
                current_epoch,
                &all_sectors,
                &pledge_penalty_for_sector_termination,
            )
            .unwrap();
        }
        Ok(())
    })?;

    // End any fault state before terminating sector power.
    // TODO: could we compress the three calls to power actor into one sector termination call?
    request_end_faults(rt, state.info.sector_size, faulty_sectors);
    request_terminate_deals(rt, deal_ids);
    request_terminate_power(rt, termination_type, state.info.sector_size, all_sectors);

    burn_funds_and_notify_pledge_change(rt, &penalty);

    Ok(())
}

/// Removes a group sectors from the sector set and its number from all sector collections in state.
fn remove_terminated_sectors<BS>(
    st: &mut State,
    store: &BS,
    deadlines: &mut Deadlines,
    sectors: &mut BitField,
) -> Result<(), String>
where
    BS: BlockStore,
{
    st.delete_sector(store, sectors)?;
    st.remove_new_sectors(sectors)?;
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
    sectors: Vec<SectorOnChainInfo>,
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if sectors.is_empty() {
        return Ok(());
    }

    let mut param = OnFaultBeginParams {
        weights: Vec::with_capacity(sectors.len()),
    };
    for (i, s) in sectors.iter().enumerate() {
        param.weights[i] = as_storage_weight_desc(&sector_size, s);
    }
    let ser_params = Serialized::serialize(param)?;

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
    sectors: Vec<SectorOnChainInfo>,
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if sectors.is_empty() {
        return Ok(());
    }

    let mut param = OnFaultEndParams {
        weights: Vec::with_capacity(sectors.len()),
    };
    for (i, s) in sectors.iter().enumerate() {
        param.weights[i] = as_storage_weight_desc(&sector_size, s);
    }
    let ser_params = Serialized::serialize(param)?;

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

    let ser_params = Serialized::serialize(OnMinerSectorsTerminateParams { deal_ids })?;

    rt.send(
        &*STORAGE_MARKET_ACTOR_ADDR,
        MarketMethod::OnMinerSectorsTerminate as u64,
        &ser_params,
        &TokenAmount::zero(),
    )?;
    Ok(())
}
fn request_terminate_power<BS, RT>(
    rt: &mut RT,
    termination_type: SectorTermination,
    sector_size: SectorSize,
    sectors: Vec<SectorOnChainInfo>,
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if sectors.is_empty() {
        return Ok(());
    }

    let mut param = OnSectorTerminateParams {
        termination_type,
        weights: Vec::with_capacity(sectors.len()),
    };
    for (i, s) in sectors.iter().enumerate() {
        param.weights[i] = as_storage_weight_desc(&sector_size, s);
    }
    let ser_params = Serialized::serialize(param)?;

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
    proofs: &[PoStProof],
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let miner_actor_id: u64 = if let Payload::ID(i) = rt.message().to().payload() {
        *i
    } else {
        panic!("could not provide ID address");
    };

    // Regenerate challenge randomness, which must match that generated for the proof.
    let entropy: &[u8] = &*rt.message().to().to_bytes();
    let post_randomness: PoStRandomness =
        rt.get_randomness(WindowedPoStChallengeSeed, challenge_epoch, entropy)?;

    let mut sector_proof_info: Vec<SectorInfo> = Vec::with_capacity(sectors.len());
    for (i, s) in sectors.iter().enumerate() {
        sector_proof_info[i] = s.as_sector_info();
    }

    // get public inputs
    let pv_info = WindowPoStVerifyInfo {
        randomness: post_randomness,
        proofs: proofs.to_vec(),
        challenged_sectors: sector_proof_info,
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

    let commd =
        request_unsealed_sector_cid(rt, params.registered_proof, params.deal_ids.clone()).unwrap(); // handle err

    let miner_actor_id: u64 = if let Payload::ID(i) = rt.message().to().payload() {
        *i
    } else {
        panic!("could not provide ID address");
    };
    let entropy: &[u8] = &*rt.message().to().to_bytes();
    let sv_info_randomness: SealRandom =
        rt.get_randomness(SealRandomness, params.seal_rand_epoch, entropy)?;
    let sv_info_interactive_randomness: InteractiveSealRandomness = rt.get_randomness(
        InteractiveSealChallengeSeed,
        params.interactive_epoch,
        entropy,
    )?;

    Ok(SealVerifyInfo {
        registered_proof: params.registered_proof,
        sector_id: SectorID {
            miner: miner_actor_id,
            number: params.sector_num,
        },
        deal_ids: params.deal_ids,
        interactive_randomness: sv_info_interactive_randomness,
        proof: params.proof,
        randomness: sv_info_randomness,
        sealed_cid: params.sealed_cid,
        unsealed_cid: commd,
    })
}
/// Requests the storage market actor compute the unsealed sector CID from a sector's deals.
fn request_unsealed_sector_cid<BS, RT>(
    rt: &mut RT,
    proof_type: RegisteredProof,
    deal_ids: Vec<DealID>,
) -> Result<Cid, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let ser_params = Serialized::serialize(ComputeDataCommitmentParams {
        sector_type: proof_type,
        deal_ids,
    })?;
    let ret = rt.send(
        &*STORAGE_MARKET_ACTOR_ADDR,
        MarketMethod::ComputeDataCommitment as u64,
        &ser_params,
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
    let current_epoch = rt.curr_epoch();
    rt.transaction(|st: &mut State, _| {
        if st.info.pending_worker_key.is_none() {
            return Err(ActorError::new(
                ExitCode::ErrIllegalState,
                "No pending key change.".to_string(),
            ));
        }
        // todo deal with unwrap
        if let Some(worker_key) = &st.info.pending_worker_key {
            if worker_key.effective_at > current_epoch {
                return Err(ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!(
                        "Too early for key change. Current: {}, Change: {}",
                        current_epoch, worker_key.effective_at
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
        let pub_key: Address = ret.deserialize().map_err(|_| {
            ActorError::new(
                ExitCode::ErrSerialization,
                format!("failed to deserialize address result: {:?}", ret),
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

fn burn_funds_and_notify_pledge_change<BS, RT>(rt: &mut RT, amount: &TokenAmount)
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    burn_funds(rt, amount);
    notify_pledge_change(rt, amount);
}
fn burn_funds<BS, RT>(rt: &mut RT, amount: &TokenAmount) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if amount > &BigUint::zero() {
        rt.send(
            &*BURNT_FUNDS_ACTOR_ADDR,
            METHOD_SEND,
            &Serialized::default(),
            amount,
        )?;
    }
    Ok(())
}
fn notify_pledge_change<BS, RT>(rt: &mut RT, pledge_delta: &TokenAmount) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if !pledge_delta.is_zero() {
        rt.send(
            &*STORAGE_POWER_ACTOR_ADDR,
            PowerMethod::UpdatePledgeTotal as u64,
            &Serialized::serialize(BigUintSer(pledge_delta))?,
            &TokenAmount::zero(),
        )?;
    }
    Ok(())
}
/// Assigns proving period offset randomly in the range [0, WPoStProvingPeriod) by hashing
/// the actor's address and current epoch.
fn assign_proving_period_offset(
    _addr: Address,
    _current_epoch: ChainEpoch,
    _syscall: &dyn Syscalls,
) -> Result<ChainEpoch, ActorError> {
    // actor err should be errSerialization exit code; msg = failed to assign proving period offset
    todo!()
}

/// Computes the epoch at which a proving period should start such that it is greater than the current epoch, and
/// has a defined offset from being an exact multiple of WPoStProvingPeriod.
/// A miner is exempt from Winow PoSt until the first full proving period starts.
fn next_proving_period_start(
    current_epoch: ChainEpoch,
    offset: ChainEpoch,
) -> Result<ChainEpoch, String> {
    let curr_modulus = current_epoch % WPOST_PROVING_PERIOD;

    let period_progress = if curr_modulus >= offset {
        curr_modulus - offset
    } else {
        WPOST_PROVING_PERIOD - (offset - curr_modulus)
    };

    let period_start = current_epoch - (period_progress + WPOST_PROVING_PERIOD);
    assert!(period_start > current_epoch);
    Ok(period_start)
}

/// Computes deadline information for a fault or recovery declaration.
/// If the deadline has not yet elapsed, the declaration is taken as being for the current proving period.
/// If the deadline has elapsed, it's instead taken as being for the next proving period after the current epoch.
fn declaration_deadline_info(
    period_start: ChainEpoch,
    deadline_idx: u64,
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
    mut declared_sectors: &mut BitField,
) -> Result<(), String> {
    if deadline.fault_cutoff_passed() {
        return Err("late fault or recovery declaration".to_string());
    }

    // check that the declared sectors are actually due at the deadline
    let deadline_sectors = deadlines
        .due
        .get_mut(deadline.index as usize)
        .ok_or("deadline not found")?;
    let contains = deadline_sectors.contains_all(&mut declared_sectors)?;
    if !contains {
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
    current_epoch: &ChainEpoch,
    sectors: &[SectorOnChainInfo],
    f: &dyn Fn(&SectorOnChainInfo) -> TokenAmount,
) -> Result<TokenAmount, String>
where
    BS: BlockStore,
{
    let mut fee = BigUint::zero();
    for s in sectors {
        fee += f(s)
    }

    st.unlock_unvested_funds(store, *current_epoch, fee)
}

/// The oldest seal challenge epoch that will be accepted in the current epoch.
fn seal_challenge_earliest(current_epoch: ChainEpoch, proof: RegisteredProof) -> ChainEpoch {
    current_epoch - CHAIN_FINALITYISH - max_seal_duration(&proof).unwrap()
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
                let BigUintDe(param) = params.deserialize()?;
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
            _ => {
                // Method number does not match available, abort in runtime
                Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method".to_owned()))
            }
        }
    }
}
