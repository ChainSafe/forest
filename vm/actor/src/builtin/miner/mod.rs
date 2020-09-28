// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod bitfield_queue;
mod deadline_assignment;
mod deadline_state;
mod deadlines;
mod expiration_queue;
mod monies;
mod partition_state;
mod policy;
mod quantize;
mod sector_map;
mod sectors;
mod state;
mod termination;
mod types;
mod vesting_state;

pub use bitfield_queue::*;
pub use deadline_assignment::*;
pub use deadline_state::*;
pub use deadlines::*;
pub use expiration_queue::*;
pub use monies::*;
pub use partition_state::*;
pub use policy::*;
pub use quantize::*;
pub use sector_map::*;
pub use sectors::*;
pub use state::*;
pub use termination::*;
pub use types::*;
pub use vesting_state::*;

use crate::{account::Method as AccountMethod, actor_error, market::ActivateDealsParams};
use crate::{
    check_empty_params, is_principal, make_map, smooth::FilterEstimate, ACCOUNT_ACTOR_CODE_ID,
    BURNT_FUNDS_ACTOR_ADDR, CALLER_TYPES_SIGNABLE, INIT_ACTOR_ADDR, REWARD_ACTOR_ADDR,
    STORAGE_MARKET_ACTOR_ADDR, STORAGE_POWER_ACTOR_ADDR,
};
use crate::{
    market::{
        ComputeDataCommitmentParamsRef, Method as MarketMethod, OnMinerSectorsTerminateParams,
        OnMinerSectorsTerminateParamsRef, VerifyDealsForActivationParamsRef,
        VerifyDealsForActivationReturn,
    },
    power::CurrentTotalPowerReturn,
};
use crate::{
    power::{EnrollCronEventParams, Method as PowerMethod},
    reward::ThisEpochRewardReturn,
};
use address::{Address, Payload, Protocol};
use bitfield::BitField;
use byteorder::{BigEndian, ByteOrder};
use cid::{multihash::Blake2b256, Cid};
use clock::ChainEpoch;
use crypto::DomainSeparationTag::{
    self, InteractiveSealChallengeSeed, SealRandomness, WindowedPoStChallengeSeed,
};
use encoding::Cbor;
use fil_types::{
    InteractiveSealRandomness, PoStProof, PoStRandomness, RegisteredSealProof,
    SealRandomness as SealRandom, SealVerifyInfo, SealVerifyParams, SectorID, SectorInfo,
    SectorNumber, SectorSize, WindowPoStVerifyInfo, MAX_SECTOR_NUMBER,
};
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser::{BigIntDe, BigIntSer};
use num_bigint::BigInt;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Signed, Zero};
use runtime::{ActorCode, Runtime};
use std::collections::HashMap;
use std::error::Error as StdError;
use std::{cmp, iter, ops::Neg};
use vm::{
    ActorError, DealID, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR,
    METHOD_SEND,
};

// * Updated to specs-actors v0.9.3 (f4024efad09a66e32bfeef10a2845b2b35325297)

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
    CompactPartitions = 19,
    CompactSectorNumbers = 20,
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
        rt.validate_immediate_caller_is(&[*INIT_ACTOR_ADDR])?;

        if !check_supported_proof_types(params.seal_proof_type) {
            return Err(actor_error!(
                ErrIllegalArgument,
                "proof type {:?} not allowed for new miner actors",
                params.seal_proof_type
            ));
        }

        let owner = resolve_control_address(rt, params.owner)?;
        let worker = resolve_worker_address(rt, params.worker)?;
        let control_addresses: Vec<_> = params
            .control_addresses
            .into_iter()
            .map(|address| resolve_control_address(rt, address))
            .collect::<Result<_, _>>()?;

        let empty_map = make_map::<_, ()>(rt.store()).flush().map_err(|e| {
            actor_error!(ErrIllegalState, "failed to construct initial state: {}", e)
        })?;

        let empty_array = Amt::<Cid, BS>::new(rt.store()).flush().map_err(|e| {
            actor_error!(ErrIllegalState, "failed to construct initial state: {}", e)
        })?;

        let empty_bitfield_cid = rt.store().put(&BitField::new(), Blake2b256).map_err(|e| {
            ActorError::downcast(
                e,
                ExitCode::ErrIllegalState,
                "failed to construct illegal state",
            )
        })?;

        let empty_deadline_cid = rt
            .store()
            .put(&Deadline::new(empty_array.clone()), Blake2b256)
            .map_err(|e| {
                ActorError::downcast(
                    e,
                    ExitCode::ErrIllegalState,
                    "failed to construct illegal state",
                )
            })?;

        let empty_deadlines_cid = rt
            .store()
            .put(&Deadlines::new(empty_deadline_cid), Blake2b256)
            .map_err(|e| {
                ActorError::downcast(
                    e,
                    ExitCode::ErrIllegalState,
                    "failed to construct illegal state",
                )
            })?;

        let empty_vesting_funds_cid =
            rt.store()
                .put(&VestingFunds::new(), Blake2b256)
                .map_err(|e| {
                    ActorError::downcast(
                        e,
                        ExitCode::ErrIllegalState,
                        "failed to construct illegal state",
                    )
                })?;

        let current_epoch = rt.curr_epoch();
        let blake2b = |b: &[u8]| rt.syscalls().hash_blake2b(b);
        let offset = assign_proving_period_offset(*rt.message().receiver(), current_epoch, blake2b)
            .map_err(|e| {
                actor_error!(
                    ErrSerialization,
                    "failed to assign proving period offset: {}",
                    e
                )
            })?;

        let period_start = next_proving_period_start(current_epoch, offset);
        assert!(period_start > current_epoch);

        let info = MinerInfo::new(
            owner,
            worker,
            control_addresses,
            params.peer_id,
            params.multi_addresses,
            params.seal_proof_type,
        )
        .map_err(|e| {
            actor_error!(
                ErrIllegalArgument,
                "failed to construct initial miner info: {}",
                e
            )
        })?;
        let info_cid = rt.store().put(&info, Blake2b256).map_err(|e| {
            ActorError::downcast(
                e,
                ExitCode::ErrIllegalState,
                "failed to construct illegal state",
            )
        })?;

        let st = State::new(
            info_cid,
            period_start,
            empty_bitfield_cid,
            empty_array,
            empty_map,
            empty_deadlines_cid,
            empty_vesting_funds_cid,
        );
        rt.create(&st)?;

        // Register first cron callback for epoch before the first proving period starts.
        enroll_cron_event(
            rt,
            period_start - 1,
            CronEventPayload {
                event_type: CRON_EVENT_PROVING_DEADLINE,
            },
        )?;

        Ok(())
    }

    fn get_miner_info<BS>(store: &BS, state: &State) -> Result<MinerInfo, ActorError>
    where
        BS: BlockStore,
    {
        state.get_info(store).map_err(|e| {
            ActorError::downcast(e, ExitCode::ErrIllegalState, "could not read miner info")
        })
    }

    fn control_addresses<BS, RT>(rt: &mut RT) -> Result<GetControlAddressesReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        let state: State = rt.state()?;
        let info = Self::get_miner_info(rt.store(), &state)?;
        Ok(GetControlAddressesReturn {
            owner: info.owner,
            worker: info.worker,
            control_addresses: info.control_addresses,
        })
    }

    /// Will ALWAYS overwrite the existing control addresses with the control addresses passed in the params.
    /// If an empty addresses vector is passed, the control addresses will be cleared.
    /// A worker change will be scheduled if the worker passed in the params is different from the existing worker.
    fn change_worker_address<BS, RT>(
        rt: &mut RT,
        params: ChangeWorkerAddressParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let new_worker = resolve_worker_address(rt, params.new_worker)?;
        let control_addresses: Vec<Address> = params
            .new_control_addresses
            .into_iter()
            .map(|address| resolve_control_address(rt, address))
            .collect::<Result<_, _>>()?;

        let effective_epoch = rt.transaction(|state: &mut State, rt| {
            let mut info = Self::get_miner_info(rt.store(), state)?;

            // Only the Owner is allowed to change the new_worker and control addresses.
            rt.validate_immediate_caller_is(&[info.owner])?;

            // save the new control addresses
            info.control_addresses = control_addresses;

            let effective_epoch = if new_worker == info.worker {
                None
            } else {
                // save new_worker addr key change request
                // This may replace another pending key change.

                let effective_epoch = rt.curr_epoch() + WORKER_KEY_CHANGE_DELAY;

                info.pending_worker_key = Some(WorkerKeyChange {
                    new_worker,
                    effective_at: effective_epoch,
                });

                Some(effective_epoch)
            };

            state.save_info(rt.store(), info).map_err(|e| {
                ActorError::downcast(e, ExitCode::ErrIllegalState, "could not save miner info")
            })?;

            Ok(effective_epoch)
        })?;

        if let Some(effective_epoch) = effective_epoch {
            let cron_payload = CronEventPayload {
                event_type: CRON_EVENT_WORKER_KEY_CHANGE,
            };
            enroll_cron_event(rt, effective_epoch, cron_payload)?;
        }

        Ok(())
    }

    fn change_peer_id<BS, RT>(rt: &mut RT, params: ChangePeerIDParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.transaction(|state: &mut State, rt| {
            let mut info = Self::get_miner_info(rt.store(), state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            info.peer_id = params.new_id;
            state.save_info(rt.store(), info).map_err(|e| {
                ActorError::downcast(e, ExitCode::ErrIllegalState, "could not save miner info")
            })?;

            Ok(())
        })?;
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
        rt.transaction(|state: &mut State, rt| {
            let mut info = Self::get_miner_info(rt.store(), state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            info.multi_address = params.new_multi_addrs;
            state.save_info(rt.store(), info).map_err(|e| {
                ActorError::downcast(e, ExitCode::ErrIllegalState, "could not save miner info")
            })?;

            Ok(())
        })?;
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

        if params.deadline >= WPOST_PERIOD_DEADLINES {
            return Err(actor_error!(
                ErrIllegalArgument,
                "invalid deadline {} of {}",
                params.deadline,
                WPOST_PERIOD_DEADLINES
            ));
        }

        if params.chain_commit_epoch >= current_epoch {
            return Err(actor_error!(
                ErrIllegalArgument,
                "PoSt chain commitment {} must be in the past",
                params.chain_commit_epoch
            ));
        }

        if params.chain_commit_epoch < current_epoch - WPOST_MAX_CHAIN_COMMIT_AGE {
            return Err(actor_error!(
                ErrIllegalArgument,
                "PoSt chain commitment {} too far in the past, must be after {}",
                params.chain_commit_epoch,
                current_epoch - WPOST_MAX_CHAIN_COMMIT_AGE
            ));
        }

        let comm_rand = rt.get_randomness_from_tickets(
            DomainSeparationTag::PoStChainCommit,
            params.chain_commit_epoch,
            &[],
        )?;

        if comm_rand != params.chain_commit_rand {
            return Err(actor_error!(
                ErrIllegalArgument,
                "post commit randomness mismatched"
            ));
        }

        // Get the total power/reward. We need these to compute penalties.
        let reward_stats = request_current_epoch_block_reward(rt)?;
        let power_total = request_current_total_power(rt)?;

        let mut penalty_total = TokenAmount::zero();
        let mut pledge_delta = TokenAmount::zero();

        let post_result = rt.transaction(|state: &mut State, rt| {
            let info = get_miner_info(rt, state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            // Validate that the miner didn't try to prove too many partitions at once.
            let submission_partition_limit =
                load_partitions_sectors_max(info.window_post_partition_sectors);
            if params.partitions.len() as u64 > submission_partition_limit {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "too many partitions {}, limit {}",
                    params.partitions.len(),
                    submission_partition_limit
                ));
            }

            // Load and check deadline.
            let current_deadline = state.deadline_info(current_epoch);
            let mut deadlines = state
                .load_deadlines(rt.store())
                .map_err(|e| e.wrap("failed to load deadlines"))?;

            // Check that the miner state indicates that the current proving deadline has started.
            // This should only fail if the cron actor wasn't invoked, and matters only in case that it hasn't been
            // invoked for a whole proving period, and hence the missed PoSt submissions from the prior occurrence
            // of this deadline haven't been processed yet.
            if !current_deadline.is_open() {
                return Err(actor_error!(
                    ErrIllegalState,
                    "proving period {} not yet open at {}",
                    current_deadline.period_start,
                    current_epoch
                ));
            }

            // The miner may only submit a proof for the current deadline.
            if params.deadline != current_deadline.index {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "invalid deadline {} at epoch {}, expected {}",
                    params.deadline,
                    current_epoch,
                    current_deadline.index
                ));
            }

            let sectors = Sectors::load(rt.store(), &state.sectors)
                .map_err(|e| actor_error!(ErrIllegalState, "failed to load sectors: {:?}", e))?;

            let mut deadline = deadlines
                .load_deadline(rt.store(), params.deadline)
                .map_err(|e| e.wrap(format!("failed to load deadline {}", params.deadline)))?;

            // Record proven sectors/partitions, returning updates to power and the final set of sectors
            // proven/skipped.
            //
            // NOTE: This function does not actually check the proofs but does assume that they'll be
            // successfully validated. The actual proof verification is done below in verifyWindowedPost.
            //
            // If proof verification fails, the this deadline MUST NOT be saved and this function should
            // be aborted.
            let fault_expiration = current_deadline.last() + FAULT_MAX_AGE;
            let post_result = deadline
                .record_proven_sectors(
                    rt.store(),
                    &sectors,
                    info.sector_size,
                    current_deadline.quant_spec(),
                    fault_expiration,
                    &params.partitions,
                )
                .map_err(|e| {
                    ActorError::downcast(
                        e,
                        ExitCode::ErrIllegalState,
                        format!(
                            "failed to process post submission for deadline {}",
                            params.deadline
                        ),
                    )
                })?;

            // Validate proofs

            // Load sector infos for proof, substituting a known-good sector for known-faulty sectors.
            // Note: this is slightly sub-optimal, loading info for the recovering sectors again after they were already
            // loaded above.
            let sector_infos = state
                .load_sector_infos_for_proof(
                    rt.store(),
                    &post_result.sectors,
                    &post_result.ignored_sectors,
                )
                .map_err(|e| {
                    actor_error!(
                        ErrIllegalState,
                        "failed to load proven sector info: {:?}",
                        e
                    )
                })?;

            // Skip verification if all sectors are faults.
            // We still need to allow this call to succeed so the miner can declare a whole partition as skipped.
            if !sector_infos.is_empty() {
                // Verify the proof.
                // A failed verification doesn't immediately cause a penalty; the miner can try again.
                verify_windowed_post(rt, current_deadline.challenge, &sector_infos, params.proofs)?;
            }

            // Penalize new skipped faults and retracted recoveries as undeclared faults.
            // These pay a higher fee than faults declared before the deadline challenge window opened.
            let undeclared_penalty_power = post_result.penalty_power();
            let mut undeclared_penalty_target = pledge_penalty_for_undeclared_fault(
                &reward_stats.this_epoch_reward_smoothed,
                &power_total.quality_adj_power_smoothed,
                &undeclared_penalty_power.qa,
            );

            // Subtract the "ongoing" fault fee from the amount charged now, since it will be charged at
            // the end-of-deadline cron.
            undeclared_penalty_target -= pledge_penalty_for_declared_fault(
                &reward_stats.this_epoch_reward_smoothed,
                &power_total.quality_adj_power_smoothed,
                &undeclared_penalty_power.qa,
            );

            // Penalize recoveries as declared faults (a lower fee than the undeclared, above).
            // It sounds odd, but because faults are penalized in arrears, at the _end_ of the faulty period, we must
            // penalize recovered sectors here because they won't be penalized by the end-of-deadline cron for the
            // immediately-prior faulty period.
            let declared_penalty_target = pledge_penalty_for_declared_fault(
                &reward_stats.this_epoch_reward_smoothed,
                &power_total.quality_adj_power_smoothed,
                &post_result.recovered_power.qa,
            );

            // Note: We could delay this charge until end of deadline, but that would require more accounting state.
            let total_penalty_target = undeclared_penalty_target + declared_penalty_target;
            let unlocked_balance = state.get_unlocked_balance(&rt.current_balance()?);
            let (vesting_penalty_total, balance_penalty_total) = state
                .penalize_funds_in_priority_order(
                    rt.store(),
                    current_epoch,
                    &total_penalty_target,
                    &unlocked_balance,
                )
                .map_err(|e| {
                    ActorError::downcast(
                        e,
                        ExitCode::ErrIllegalState,
                        format!(
                            "failed to unlock penalty for {:?}",
                            undeclared_penalty_power
                        ),
                    )
                })?;
            penalty_total = &vesting_penalty_total + balance_penalty_total;
            pledge_delta -= vesting_penalty_total;

            let deadline_idx = params.deadline;
            deadlines
                .update_deadline(rt.store(), params.deadline, &deadline)
                .map_err(|e| {
                    ActorError::downcast(
                        e,
                        ExitCode::ErrIllegalState,
                        format!("failed to update deadline {}", deadline_idx),
                    )
                })?;

            state.save_deadlines(rt.store(), deadlines).map_err(|e| {
                ActorError::downcast(e, ExitCode::ErrIllegalState, "failed to save deadlines")
            })?;

            Ok(post_result)
        })?;

        // Restore power for recovered sectors. Remove power for new faults.
        // NOTE: It would be permissible to delay the power loss until the deadline closes, but that would require
        // additional accounting state.
        // https://github.com/filecoin-project/specs-actors/issues/414
        request_update_power(rt, post_result.power_delta())?;

        // Burn penalties.
        burn_funds(rt, penalty_total)?;
        notify_pledge_changed(rt, &pledge_delta)?;

        Ok(())
    }

    /// Proposals must be posted on chain via sma.PublishStorageDeals before PreCommitSector.
    /// Optimization: PreCommitSector could contain a list of deals that are not published yet.
    fn pre_commit_sector<BS, RT>(rt: &mut RT, params: SectorPreCommitInfo) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if !check_supported_proof_types(params.seal_proof) {
            return Err(actor_error!(
                ErrIllegalArgument,
                "unsupported seal proof type: {:?}",
                params.seal_proof
            ));
        }

        #[allow(clippy::absurd_extreme_comparisons)]
        if params.sector_number > MAX_SECTOR_NUMBER {
            return Err(actor_error!(
                ErrIllegalArgument,
                "sector number {} out of range 0..(2^63-1)",
                params.sector_number
            ));
        }

        if params.sealed_cid.prefix() != SEALED_CID_PREFIX {
            return Err(actor_error!(
                ErrIllegalArgument,
                "sealed CID had wrong prefix"
            ));
        }

        if params.seal_rand_epoch >= rt.curr_epoch() {
            return Err(actor_error!(
                ErrIllegalArgument,
                "seal challenge epoch {} must be before now {}",
                params.seal_rand_epoch,
                rt.curr_epoch()
            ));
        }

        let challenge_earliest = seal_challenge_earliest(rt.curr_epoch(), params.seal_proof);
        if params.seal_rand_epoch < challenge_earliest {
            // The subsequent commitment proof can't possibly be accepted because the seal challenge will be deemed
            // too old. Note that passing this check doesn't guarantee the proof will be soon enough, depending on
            // when it arrives.
            return Err(actor_error!(
                ErrIllegalArgument,
                "seal challenge epoch {} too old, must be after {}",
                params.seal_rand_epoch,
                challenge_earliest
            ));
        }

        if params.expiration <= rt.curr_epoch() {
            return Err(actor_error!(
                ErrIllegalArgument,
                "sector expiration {} must be after now ({})",
                params.expiration,
                rt.curr_epoch()
            ));
        }

        if params.replace_capacity && params.deal_ids.is_empty() {
            return Err(actor_error!(
                ErrIllegalArgument,
                "cannot replace sector without committing deals"
            ));
        }

        if params.replace_sector_deadline >= WPOST_PERIOD_DEADLINES {
            return Err(actor_error!(
                ErrIllegalArgument,
                "invalid deadline {}",
                params.replace_sector_deadline
            ));
        }

        #[allow(clippy::absurd_extreme_comparisons)]
        if params.replace_sector_number >= MAX_SECTOR_NUMBER {
            return Err(actor_error!(
                ErrIllegalArgument,
                "invalid sector number {}",
                params.replace_sector_number
            ));
        }

        // gather information from other actors

        let reward_stats = request_current_epoch_block_reward(rt)?;
        let power_total = request_current_total_power(rt)?;
        let deal_weight =
            request_deal_weight(rt, &params.deal_ids, rt.curr_epoch(), params.expiration)?;

        let newly_vested = rt.transaction(|state: &mut State, rt| {
            let info = get_miner_info(rt, state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            let store = rt.store();

            if params.seal_proof != info.seal_proof_type {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "sector seal proof {:?} must match miner seal proof type {:?}",
                    params.seal_proof,
                    info.seal_proof_type
                ));
            }

            let max_deal_limit = deal_per_sector_limit(info.sector_size);
            if params.deal_ids.len() as u64 > max_deal_limit {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "too many deals for sector {} > {}",
                    params.deal_ids.len(),
                    max_deal_limit
                ));
            }

            state
                .allocate_sector_number(store, params.sector_number)
                .map_err(|e| {
                    e.wrap(format!(
                        "failed to allocate sector id {}",
                        params.sector_number
                    ))
                })?;

            // The following two checks shouldn't be necessary, but it can't
            // hurt to double-check (unless it's really just too
            // expensive?).
            let sector = state
                .get_precommitted_sector(store, params.sector_number)
                .map_err(|e| {
                    actor_error!(
                        ErrIllegalState,
                        "failed to check pre-commit {}: {:?}",
                        params.sector_number,
                        e
                    )
                })?;

            if sector.is_some() {
                return Err(actor_error!(
                    ErrIllegalState,
                    "sector {} already pre-committed",
                    params.sector_number
                ));
            }

            let sector_found = state
                .has_sector_number(store, params.sector_number)
                .map_err(|e| {
                    actor_error!(
                        ErrIllegalState,
                        "failed to check sector {}: {}",
                        params.sector_number,
                        e
                    )
                })?;

            if sector_found {
                return Err(actor_error!(
                    ErrIllegalState,
                    "sector {} already committed",
                    params.sector_number
                ));
            }

            // Require sector lifetime meets minimum by assuming activation happens at last epoch permitted for seal proof.
            // This could make sector maximum lifetime validation more lenient if the maximum sector limit isn't hit first.
            let max_activation = rt.curr_epoch() + max_seal_duration(params.seal_proof).unwrap();
            validate_expiration(rt, max_activation, params.expiration, params.seal_proof)?;

            let deposit_minimum = if params.replace_capacity {
                let replace_sector = validate_replace_sector(state, store, &params)?;

                // Note the replaced sector's initial pledge as a lower bound for the new sector's deposit
                replace_sector.initial_pledge
            } else {
                TokenAmount::zero()
            };

            let newly_vested = state
                .unlock_vested_funds(store, rt.curr_epoch())
                .map_err(|e| {
                    ActorError::downcast(e, ExitCode::ErrIllegalState, "failed to vest funds")
                })?;

            let available_balance = state.get_available_balance(&rt.current_balance()?);
            let duration = params.expiration - rt.curr_epoch();

            let sector_weight = qa_power_for_weight(
                info.sector_size,
                duration,
                &deal_weight.deal_weight,
                &deal_weight.verified_deal_weight,
            );

            let deposit_req = cmp::max(
                pre_commit_deposit_for_power(
                    &reward_stats.this_epoch_reward_smoothed,
                    &power_total.quality_adj_power_smoothed,
                    &sector_weight,
                ),
                deposit_minimum,
            );

            if available_balance < deposit_req {
                return Err(actor_error!(
                    ErrInsufficientFunds,
                    "insufficient funds for pre-commit deposit: {}",
                    deposit_req
                ));
            }

            state.add_pre_commit_deposit(&deposit_req);
            state.assert_balance_invariants(&rt.current_balance()?);

            let seal_proof = params.seal_proof;
            let sector_number = params.sector_number;

            state
                .put_precommitted_sector(
                    store,
                    SectorPreCommitOnChainInfo {
                        info: params,
                        pre_commit_deposit: deposit_req,
                        pre_commit_epoch: rt.curr_epoch(),
                        deal_weight: deal_weight.deal_weight,
                        verified_deal_weight: deal_weight.verified_deal_weight,
                    },
                )
                .map_err(|e| {
                    actor_error!(
                        ErrIllegalState,
                        "failed to write pre-committed sector {}: {:?}",
                        sector_number,
                        e
                    )
                })?;

            // add precommit expiry to the queue
            let max_seal_duration = max_seal_duration(seal_proof).ok_or_else(|| {
                actor_error!(
                    ErrIllegalArgument,
                    "no max seal duration set for proof type: {:?}",
                    seal_proof
                )
            })?;

            // The +1 here is critical for the batch verification of proofs. Without it, if a proof arrived exactly on the
            // due epoch, ProveCommitSector would accept it, then the expiry event would remove it, and then
            // ConfirmSectorProofsValid would fail to find it.
            let expiry_bound = rt.curr_epoch() + max_seal_duration + 1;

            state
                .add_pre_commit_expiry(store, expiry_bound, sector_number)
                .map_err(|e| {
                    ActorError::downcast(
                        e,
                        ExitCode::ErrIllegalState,
                        "failed to add pre-commit expiry to queue",
                    )
                })?;

            Ok(newly_vested)
        })?;

        notify_pledge_changed(rt, &-newly_vested)?;
        Ok(())
    }

    /// Checks state of the corresponding sector pre-commitment, then schedules the proof to be verified in bulk
    /// by the power actor.
    /// If valid, the power actor will call ConfirmSectorProofsValid at the end of the same epoch as this message.
    fn prove_commit_sector<BS, RT>(
        rt: &mut RT,
        params: ProveCommitSectorParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;

        let st: State = rt.state()?;

        // Verify locked funds are are at least the sum of sector initial pledges.
        // Note that this call does not actually compute recent vesting, so the reported locked funds may be
        // slightly higher than the true amount (i.e. slightly in the miner's favour).
        // Computing vesting here would be almost always redundant since vesting is quantized to ~daily units.
        // Vesting will be at most one proving period old if computed in the cron callback.
        verify_pledge_meets_initial_requirements(rt, &st)?;

        let sector_number = params.sector_number;
        let precommit = st
            .get_precommitted_sector(rt.store(), sector_number)
            .map_err(|e| {
                actor_error!(
                    ErrIllegalState,
                    "failed to load precommitted sector: {}, {}",
                    sector_number,
                    e
                )
            })?
            .ok_or_else(|| {
                actor_error!(ErrNotFound, "no pre-committed sector: {}", sector_number)
            })?;

        let msd = max_seal_duration(precommit.info.seal_proof).ok_or_else(|| {
            actor_error!(
                ErrIllegalState,
                "no max seal duration set for proof type: {:?}",
                precommit.info.seal_proof
            )
        })?;
        let prove_commit_due = precommit.pre_commit_epoch + msd;
        if rt.curr_epoch() > prove_commit_due {
            return Err(actor_error!(
                ErrIllegalArgument,
                "commitment proof for {} too late at {}, due {}",
                sector_number,
                rt.curr_epoch(),
                prove_commit_due
            ));
        }

        let svi = get_verify_info(
            rt,
            SealVerifyParams {
                sealed_cid: precommit.info.sealed_cid.clone(),
                interactive_epoch: precommit.pre_commit_epoch + PRE_COMMIT_CHALLENGE_DELAY,
                seal_rand_epoch: precommit.info.seal_rand_epoch,
                proof: params.proof,
                deal_ids: precommit.info.deal_ids.clone(),
                sector_num: precommit.info.sector_number,
                registered_seal_proof: precommit.info.seal_proof,
            },
        )?;

        rt.send(
            *STORAGE_POWER_ACTOR_ADDR,
            PowerMethod::SubmitPoRepForBulkVerify as u64,
            Serialized::serialize(&svi)?,
            BigInt::zero(),
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
        rt.validate_immediate_caller_is(iter::once(&*STORAGE_POWER_ACTOR_ADDR))?;

        // get network stats from other actors
        let reward_stats = request_current_epoch_block_reward(rt)?;
        let power_total = request_current_total_power(rt)?;
        let circulating_supply = rt.total_fil_circ_supply()?;

        // 1. Activate deals, skipping pre-commits with invalid deals.
        //    - calls the market actor.
        // 2. Reschedule replacement sector expiration.
        //    - loads and saves sectors
        //    - loads and saves deadlines/partitions
        // 3. Add new sectors.
        //    - loads and saves sectors.
        //    - loads and saves deadlines/partitions
        //
        // Ideally, we'd combine some of these operations, but at least we have
        // a constant number of them.

        let state = rt.state()?;
        let info = get_miner_info(rt, &state)?;

        //
        // Activate storage deals.
        //

        // This skips missing pre-commits.
        let precommitted_sectors = state
            .find_precommitted_sectors(rt.store(), &params.sectors)
            .map_err(|e| {
                actor_error!(
                    ErrIllegalState,
                    "failed to load pre-committed sectors: {}",
                    e
                )
            })?;

        // Committed-capacity sectors licensed for early removal by new sectors being proven.
        let mut replace_sectors = DeadlineSectorMap::new();

        // Pre-commits for new sectors.
        let mut pre_commits = Vec::<SectorPreCommitOnChainInfo>::new();

        for pre_commit in precommitted_sectors {
            // Check (and activate) storage deals associated to sector. Abort if checks failed.
            let res = rt.send(
                *STORAGE_MARKET_ACTOR_ADDR,
                crate::market::Method::ActivateDeals as MethodNum,
                Serialized::serialize(ActivateDealsParams {
                    deal_ids: pre_commit.info.deal_ids.clone(),
                    sector_expiry: pre_commit.info.expiration,
                })?,
                TokenAmount::zero(),
            );

            if let Err(e) = res {
                log::info!(
                    "failed to activate deals on sector {}, dropping from prove commit set: {}",
                    pre_commit.info.sector_number,
                    e.msg()
                );
                continue;
            }

            if pre_commit.info.replace_capacity {
                replace_sectors
                    .add_values(
                        pre_commit.info.replace_sector_deadline,
                        pre_commit.info.replace_sector_partition,
                        &[pre_commit.info.replace_sector_number],
                    )
                    .map_err(|e| {
                        actor_error!(
                            ErrIllegalArgument,
                            "failed to record sectors for replacement: {}",
                            e
                        )
                    })?;
            }

            pre_commits.push(pre_commit);
        }

        // When all prove commits have failed abort early
        if pre_commits.is_empty() {
            return Err(actor_error!(
                ErrIllegalArgument,
                "all prove commits failed to validate"
            ));
        }

        let (new_power, total_pledge, newly_vested) = rt.transaction(|state: &mut State, rt| {
            let store = rt.store();

            // Schedule expiration for replaced sectors to the end of their next deadline window.
            // They can't be removed right now because we want to challenge them immediately before termination.
            state
                .reschedule_sector_expirations(
                    store,
                    rt.curr_epoch(),
                    info.sector_size,
                    replace_sectors,
                )
                .map_err(|e| {
                    ActorError::downcast(
                        e,
                        ExitCode::ErrIllegalState,
                        "failed to replace sector expirations",
                    )
                })?;

            let mut new_sector_numbers = Vec::<SectorNumber>::with_capacity(pre_commits.len());
            let mut total_pre_commit_deposit = TokenAmount::zero();
            let mut new_sectors = Vec::<SectorOnChainInfo>::new();
            let mut total_pledge = TokenAmount::zero();

            for pre_commit in pre_commits {
                // compute initial pledge
                let activation = rt.curr_epoch();
                let duration = pre_commit.info.expiration - activation;

                // This should have been caught in precommit, but don't let other sectors fail because of it.
                if duration < MIN_SECTOR_EXPIRATION {
                    log::warn!(
                        "precommit {} has lifetime {} less than minimum {}. ignoring",
                        pre_commit.info.sector_number,
                        duration,
                        MIN_SECTOR_EXPIRATION,
                    );
                }

                let power = qa_power_for_weight(
                    info.sector_size,
                    duration,
                    &pre_commit.deal_weight,
                    &pre_commit.verified_deal_weight,
                );

                let day_reward = expected_reward_for_power(
                    &reward_stats.this_epoch_reward_smoothed,
                    &power_total.quality_adj_power_smoothed,
                    &power,
                    crate::EPOCHS_IN_DAY,
                );

                // The storage pledge is recorded for use in computing the penalty if this sector is terminated
                // before its declared expiration.
                // It's not capped to 1 FIL for Space Race, so likely exceeds the actual initial pledge requirement.
                let storage_pledge = expected_reward_for_power(
                    &reward_stats.this_epoch_reward_smoothed,
                    &power_total.quality_adj_power_smoothed,
                    &power,
                    INITIAL_PLEDGE_PROJECTION_PERIOD,
                );

                let initial_pledge = initial_pledge_for_power(
                    &power,
                    &reward_stats.this_epoch_baseline_power,
                    &reward_stats.this_epoch_reward_smoothed,
                    &power_total.quality_adj_power_smoothed,
                    &circulating_supply,
                );

                total_pre_commit_deposit += &pre_commit.pre_commit_deposit;
                total_pledge += &initial_pledge;

                let new_sector_info = SectorOnChainInfo {
                    sector_number: pre_commit.info.sector_number,
                    seal_proof: pre_commit.info.seal_proof,
                    sealed_cid: pre_commit.info.sealed_cid,
                    deal_ids: pre_commit.info.deal_ids,
                    activation,
                    expiration: pre_commit.info.expiration,
                    deal_weight: pre_commit.deal_weight,
                    verified_deal_weight: pre_commit.verified_deal_weight,
                    initial_pledge,
                    expected_day_reward: day_reward,
                    expected_storage_pledge: storage_pledge,
                };

                new_sector_numbers.push(new_sector_info.sector_number);
                new_sectors.push(new_sector_info);
            }

            state
                .put_sectors(store, new_sectors.clone())
                .map_err(|e| actor_error!(ErrIllegalState, "failed to put new sectors: {}", e))?;

            state
                .delete_precommitted_sectors(store, &new_sector_numbers)
                .map_err(|e| {
                    actor_error!(
                        ErrIllegalState,
                        "failed to delete precommited sectors: {:?}",
                        e
                    )
                })?;

            let new_power = state
                .assign_sectors_to_deadlines(
                    store,
                    rt.curr_epoch(),
                    new_sectors,
                    info.window_post_partition_sectors,
                    info.sector_size,
                )
                .map_err(|e| {
                    ActorError::downcast(
                        e,
                        ExitCode::ErrIllegalState,
                        "failed to assign new sectors to deadlines",
                    )
                })?;

            // Add sector and pledge lock-up to miner state
            let newly_vested = state
                .unlock_vested_funds(store, rt.curr_epoch())
                .map_err(|e| {
                    actor_error!(
                        ErrIllegalState,
                        "failed to assign new sectors to deadlines: {:?}",
                        e
                    )
                })?;

            // Unlock deposit for successful proofs, make it available for lock-up as initial pledge.
            state.add_pre_commit_deposit(&(-total_pre_commit_deposit));

            let available_balance = state.get_available_balance(&rt.current_balance()?);
            if available_balance < total_pledge {
                return Err(actor_error!(
                    ErrInsufficientFunds,
                    "insufficient funds for aggregate initial pledge requirement {}, available: {}",
                    total_pledge,
                    available_balance
                ));
            }

            state.add_initial_pledge_requirement(&total_pledge);
            state.assert_balance_invariants(&rt.current_balance()?);

            Ok((new_power, total_pledge, newly_vested))
        })?;

        // Request power and pledge update for activated sector.
        request_update_power(rt, new_power)?;
        notify_pledge_changed(rt, &(total_pledge - newly_vested))?;

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
        rt.validate_immediate_caller_accept_any()?;
        let st: State = rt.state()?;

        match st.get_sector(rt.store(), params.sector_number) {
            Err(e) => Err(actor_error!(
                ErrIllegalState,
                "failed to load proven sector {}: {}",
                params.sector_number,
                e
            )),
            Ok(None) => Err(actor_error!(
                ErrNotFound,
                "sector {} not proven",
                params.sector_number
            )),
            Ok(Some(_sector)) => Ok(()),
        }
    }

    /// Changes the expiration epoch for a sector to a new, later one.
    /// The sector must not be terminated or faulty.
    /// The sector's power is recomputed for the new expiration.
    fn extend_sector_expiration<BS, RT>(
        rt: &mut RT,
        params: ExtendSectorExpirationParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if params.extensions.len() as u64 > ADDRESSED_PARTITIONS_MAX {
            return Err(actor_error!(
                ErrIllegalArgument,
                "too many declarations {}, max {}",
                params.extensions.len(),
                ADDRESSED_PARTITIONS_MAX
            ));
        }

        // limit the number of sectors declared at once
        // https://github.com/filecoin-project/specs-actors/issues/416
        let mut sector_count: u64 = 0;

        for decl in &params.extensions {
            if decl.deadline >= WPOST_PERIOD_DEADLINES {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "deadline {} not in range 0..{}",
                    decl.deadline,
                    WPOST_PERIOD_DEADLINES
                ));
            }

            match sector_count.checked_add(decl.sectors.len() as u64) {
                Some(sum) => sector_count = sum,
                None => {
                    return Err(actor_error!(
                        ErrIllegalArgument,
                        "sector bitfield integer overflow"
                    ));
                }
            }
        }

        if sector_count > ADDRESSED_SECTORS_MAX {
            return Err(actor_error!(
                ErrIllegalArgument,
                "too many sectors for declaration {}, max {}",
                sector_count,
                ADDRESSED_SECTORS_MAX
            ));
        }

        let (power_delta, pledge_delta) = rt.transaction(|state: &mut State, rt| {
            let info = get_miner_info(rt, state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            let store = rt.store();

            let mut deadlines = state
                .load_deadlines(rt.store())
                .map_err(|e| e.wrap("failed to load deadlines"))?;

            // Group declarations by deadline, and remember iteration order.
            let mut decls_by_deadline = HashMap::<u64, Vec<&ExpirationExtension>>::new();
            let mut deadlines_to_load = Vec::<u64>::new();

            for decl in &params.extensions {
                decls_by_deadline
                    .entry(decl.deadline)
                    .or_insert_with(|| {
                        deadlines_to_load.push(decl.deadline);
                        Vec::new()
                    })
                    .push(decl);
            }

            let mut sectors = Sectors::load(rt.store(), &state.sectors).map_err(|e| {
                actor_error!(ErrIllegalState, "failed to load sectors array: {:?}", e)
            })?;

            let mut power_delta = PowerPair::zero();
            let mut pledge_delta = TokenAmount::zero();

            for deadline_idx in deadlines_to_load {
                let mut deadline = deadlines
                    .load_deadline(store, deadline_idx)
                    .map_err(|e| e.wrap(format!("failed to load deadline {}", deadline_idx)))?;

                let mut partitions = deadline.partitions_amt(store).map_err(|e| {
                    e.wrap(format!(
                        "failed to load partitions for deadline {}",
                        deadline_idx
                    ))
                })?;

                let quant = state.quant_spec_for_deadline(deadline_idx);

                for &decl in &decls_by_deadline[&deadline_idx] {
                    let key = PartitionKey {
                        deadline: deadline_idx,
                        partition: decl.partition,
                    };

                    let mut partition = partitions
                        .get(decl.partition)
                        .map_err(|e| {
                            actor_error!(
                                ErrIllegalState,
                                "failed to load partition {:?}: {:?}",
                                key,
                                e
                            )
                        })?
                        .cloned()
                        .ok_or_else(|| actor_error!(ErrNotFound, "no such partition {:?}", key))?;

                    let old_sectors = sectors
                        .load_sector(&decl.sectors)
                        .map_err(|e| e.wrap("failed to load sectors"))?;

                    let new_sectors: Vec<SectorOnChainInfo> = old_sectors
                        .iter()
                        .map(|sector| {
                            if decl.new_expiration < sector.expiration {
                                return Err(actor_error!(
                                    ErrIllegalArgument,
                                    "cannot reduce sector expiration to {} from {}",
                                    decl.new_expiration,
                                    sector.expiration
                                ));
                            }

                            validate_expiration(
                                rt,
                                sector.activation,
                                decl.new_expiration,
                                sector.seal_proof,
                            )?;

                            let mut sector = sector.clone();
                            sector.expiration = decl.new_expiration;
                            Ok(sector)
                        })
                        .collect::<Result<_, _>>()?;

                    // Overwrite sector infos.
                    sectors.store(new_sectors.clone()).map_err(|e| {
                        actor_error!(
                            ErrIllegalState,
                            "failed to update sectors {:?}: {}",
                            decl.sectors,
                            e
                        )
                    })?;

                    // Remove old sectors from partition and assign new sectors.
                    let (partition_power_delta, partition_pledge_delta) = partition
                        .replace_sectors(store, &old_sectors, &new_sectors, info.sector_size, quant)
                        .map_err(|e| {
                            actor_error!(
                                ErrIllegalState,
                                "failed to replaces sector expirations at {:?}: {}",
                                key,
                                e
                            )
                        })?;

                    power_delta += &partition_power_delta;
                    pledge_delta += partition_pledge_delta; // expected to be zero, see note below.

                    partitions.set(decl.partition, partition).map_err(|e| {
                        actor_error!(
                            ErrIllegalState,
                            "failed to save partition {:?}: {:?}",
                            key,
                            e
                        )
                    })?;
                }

                deadline.partitions = partitions.flush().map_err(|e| {
                    actor_error!(
                        ErrIllegalState,
                        "failed to save partitions for deadline {}: {:?}",
                        deadline_idx,
                        e
                    )
                })?;

                deadlines
                    .update_deadline(store, deadline_idx, &deadline)
                    .map_err(|e| {
                        ActorError::downcast(
                            e,
                            ExitCode::ErrIllegalState,
                            format!("failed to save deadline {}", deadline_idx),
                        )
                    })?;
            }

            Ok((power_delta, pledge_delta))
        })?;

        request_update_power(rt, power_delta)?;

        // Note: the pledge delta is expected to be zero, since pledge is not re-calculated for the extension.
        // But in case that ever changes, we can do the right thing here.
        notify_pledge_changed(rt, &pledge_delta)?;
        Ok(())
    }

    /// Marks some sectors as terminated at the present epoch, earlier than their
    /// scheduled termination, and adds these sectors to the early termination queue.
    /// This method then processes up to AddressedSectorsMax sectors and
    /// AddressedPartitionsMax partitions from the early termination queue,
    /// terminating deals, paying fines, and returning pledge collateral. While
    /// sectors remain in this queue:
    ///
    ///  1. The miner will be unable to withdraw funds.
    ///  2. The chain will process up to AddressedSectorsMax sectors and
    ///     AddressedPartitionsMax per epoch until the queue is empty.
    ///
    /// The sectors are immediately ignored for Window PoSt proofs, and should be
    /// masked in the same way as faulty sectors. A miner terminating sectors in the
    /// current deadline must be careful to compute an appropriate Window PoSt proof
    /// for the sectors that will be active at the time the PoSt is submitted.
    ///
    /// This function may be invoked with no new sectors to explicitly process the
    /// next batch of sectors.
    fn terminate_sectors<BS, RT>(
        rt: &mut RT,
        params: TerminateSectorsParams,
    ) -> Result<TerminateSectorsReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // Note: this cannot terminate pre-committed but un-proven sectors.
        // They must be allowed to expire (and deposit burnt).

        let mut to_process = DeadlineSectorMap::new();

        for term in params.terminations {
            let deadline = term.deadline;
            let partition = term.partition;

            to_process
                .add(deadline, partition, term.sectors)
                .map_err(|e| {
                    actor_error!(
                        ErrIllegalArgument,
                        "failed to process deadline {}, partition {}: {}",
                        deadline,
                        partition,
                        e
                    )
                })?;
        }

        to_process
            .check(ADDRESSED_PARTITIONS_MAX, ADDRESSED_SECTORS_MAX)
            .map_err(|e| {
                actor_error!(
                    ErrIllegalArgument,
                    "cannot process requested parameters: {}",
                    e
                )
            })?;

        let (had_early_terminations, power_delta) = rt.transaction(|state: &mut State, rt| {
            let had_early_terminations = have_pending_early_terminations(state);

            let info = get_miner_info(rt, state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            let store = rt.store();
            let curr_epoch = rt.curr_epoch();
            let mut power_delta = PowerPair::zero();

            let mut deadlines = state
                .load_deadlines(store)
                .map_err(|e| e.wrap("failed to load deadlines"))?;

            // We're only reading the sectors, so there's no need to save this back.
            // However, we still want to avoid re-loading this array per-partition.
            let sectors = Sectors::load(store, &state.sectors)
                .map_err(|e| actor_error!(ErrIllegalState, "failed to load sectors: {:?}", e))?;

            for (deadline_idx, partition_sectors) in to_process.iter() {
                let quant = state.quant_spec_for_deadline(deadline_idx);
                let mut deadline = deadlines
                    .load_deadline(store, deadline_idx)
                    .map_err(|e| e.wrap(format!("failed to load deadline {}", deadline_idx)))?;

                let removed_power = deadline
                    .terminate_sectors(
                        store,
                        &sectors,
                        curr_epoch,
                        partition_sectors,
                        info.sector_size,
                        quant,
                    )
                    .map_err(|e| {
                        ActorError::downcast(
                            e,
                            ExitCode::ErrIllegalState,
                            format!("failed to terminate sectors in deadline {}", deadline_idx),
                        )
                    })?;

                state.early_terminations.set(deadline_idx as usize);
                power_delta -= &removed_power;

                deadlines
                    .update_deadline(store, deadline_idx, &deadline)
                    .map_err(|e| {
                        ActorError::downcast(
                            e,
                            ExitCode::ErrIllegalState,
                            format!("failed to update deadline {}", deadline_idx),
                        )
                    })?;
            }

            state.save_deadlines(store, deadlines).map_err(|e| {
                ActorError::downcast(e, ExitCode::ErrIllegalState, "failed to save deadlines")
            })?;

            Ok((had_early_terminations, power_delta))
        })?;

        // Now, try to process these sectors.
        let more = process_early_terminations(rt)?;

        if more && !had_early_terminations {
            // We have remaining terminations, and we didn't _previously_
            // have early terminations to process, schedule a cron job.
            // NOTE: This isn't quite correct. If we repeatedly fill, empty,
            // fill, and empty, the queue, we'll keep scheduling new cron
            // jobs. However, in practice, that shouldn't be all that bad.
            schedule_early_termination_work(rt)?;
        }

        request_update_power(rt, power_delta)?;
        Ok(TerminateSectorsReturn { done: !more })
    }

    fn declare_faults<BS, RT>(rt: &mut RT, params: DeclareFaultsParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let mut to_process = DeadlineSectorMap::new();

        for term in params.faults {
            let deadline = term.deadline;
            let partition = term.partition;

            to_process
                .add(deadline, partition, term.sectors)
                .map_err(|e| {
                    actor_error!(
                        ErrIllegalArgument,
                        "failed to process deadline {}, partition {}: {}",
                        deadline,
                        partition,
                        e
                    )
                })?;
        }

        to_process
            .check(ADDRESSED_PARTITIONS_MAX, ADDRESSED_SECTORS_MAX)
            .map_err(|e| {
                actor_error!(
                    ErrIllegalArgument,
                    "cannot process requested parameters: {}",
                    e
                )
            })?;

        let new_fault_power_total = rt.transaction(|state: &mut State, rt| {
            let info = get_miner_info(rt, &state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            let store = rt.store();

            let mut deadlines = state
                .load_deadlines(store)
                .map_err(|e| e.wrap("failed to load deadlines"))?;

            let sectors = Sectors::load(store, &state.sectors).map_err(|e| {
                actor_error!(ErrIllegalState, "failed to load sectors array: {:?}", e)
            })?;

            let mut new_fault_power_total = PowerPair::zero();

            for (deadline_idx, partition_map) in to_process.iter() {
                let target_deadline = declaration_deadline_info(
                    state.proving_period_start,
                    deadline_idx,
                    rt.curr_epoch(),
                )
                .map_err(|e| {
                    actor_error!(
                        ErrIllegalArgument,
                        "invalid fault declaration deadline {}: {}",
                        deadline_idx,
                        e
                    )
                })?;

                validate_fr_declaration_deadline(&target_deadline).map_err(|e| {
                    actor_error!(
                        ErrIllegalArgument,
                        "failed fault declaration at deadline {}: {}",
                        deadline_idx,
                        e
                    )
                })?;

                let mut deadline = deadlines
                    .load_deadline(store, deadline_idx)
                    .map_err(|e| e.wrap(format!("failed to load deadline {}", deadline_idx)))?;

                let fault_expiration_epoch = target_deadline.last() + FAULT_MAX_AGE;

                let new_faulty_power = deadline
                    .declare_faults(
                        store,
                        &sectors,
                        info.sector_size,
                        target_deadline.quant_spec(),
                        fault_expiration_epoch,
                        partition_map,
                    )
                    .map_err(|e| {
                        ActorError::downcast(
                            e,
                            ExitCode::ErrIllegalState,
                            format!("failed to declare faults for deadline {}", deadline_idx),
                        )
                    })?;

                deadlines
                    .update_deadline(store, deadline_idx, &deadline)
                    .map_err(|e| {
                        ActorError::downcast(
                            e,
                            ExitCode::ErrIllegalState,
                            format!("failed to store deadline {} partitions", deadline_idx),
                        )
                    })?;

                new_fault_power_total += &new_faulty_power;
            }

            state.save_deadlines(store, deadlines).map_err(|e| {
                ActorError::downcast(e, ExitCode::ErrIllegalState, "failed to save deadlines")
            })?;

            Ok(new_fault_power_total)
        })?;

        // Remove power for new faulty sectors.
        // NOTE: It would be permissible to delay the power loss until the deadline closes, but that would require
        // additional accounting state.
        // https://github.com/filecoin-project/specs-actors/issues/414
        request_update_power(rt, -new_fault_power_total)?;

        // Payment of penalty for declared faults is deferred to the deadline cron.
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
        let mut to_process = DeadlineSectorMap::new();

        for term in params.recoveries {
            let deadline = term.deadline;
            let partition = term.partition;

            to_process
                .add(deadline, partition, term.sectors)
                .map_err(|e| {
                    actor_error!(
                        ErrIllegalArgument,
                        "failed to process deadline {}, partition {}: {}",
                        deadline,
                        partition,
                        e
                    )
                })?;
        }

        to_process
            .check(ADDRESSED_PARTITIONS_MAX, ADDRESSED_SECTORS_MAX)
            .map_err(|e| {
                actor_error!(
                    ErrIllegalArgument,
                    "cannot process requested parameters: {}",
                    e
                )
            })?;

        rt.transaction(|state: &mut State, rt| {
            let info = get_miner_info(rt, &state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            let store = rt.store();

            let mut deadlines = state
                .load_deadlines(store)
                .map_err(|e| e.wrap("failed to load deadlines"))?;

            let sectors = Sectors::load(store, &state.sectors).map_err(|e| {
                actor_error!(ErrIllegalState, "failed to load sectors array: {:?}", e)
            })?;

            for (deadline_idx, partition_map) in to_process.iter() {
                let target_deadline = declaration_deadline_info(
                    state.proving_period_start,
                    deadline_idx,
                    rt.curr_epoch(),
                )
                .map_err(|e| {
                    actor_error!(
                        ErrIllegalArgument,
                        "invalid recovery declaration deadline {}: {}",
                        deadline_idx,
                        e
                    )
                })?;

                validate_fr_declaration_deadline(&target_deadline).map_err(|e| {
                    actor_error!(
                        ErrIllegalArgument,
                        "failed recovery declaration at deadline {}: {}",
                        deadline_idx,
                        e
                    )
                })?;

                let mut deadline = deadlines
                    .load_deadline(store, deadline_idx)
                    .map_err(|e| e.wrap(format!("failed to load deadline {}", deadline_idx)))?;

                deadline
                    .declare_faults_recovered(store, &sectors, info.sector_size, partition_map)
                    .map_err(|e| {
                        e.wrap(format!(
                            "failed to declare recoveries for deadline {}",
                            deadline_idx
                        ))
                    })?;

                deadlines
                    .update_deadline(store, deadline_idx, &deadline)
                    .map_err(|e| {
                        ActorError::downcast(
                            e,
                            ExitCode::ErrIllegalState,
                            format!("failed to store deadline {}", deadline_idx),
                        )
                    })?;
            }

            state.save_deadlines(store, deadlines).map_err(|e| {
                ActorError::downcast(e, ExitCode::ErrIllegalState, "failed to save deadlines")
            })?;

            Ok(())
        })?;

        // Power is not restored yet, but when the recovered sectors are successfully PoSted.
        Ok(())
    }

    /// Compacts a number of partitions at one deadline by removing terminated sectors, re-ordering the remaining sectors,
    /// and assigning them to new partitions so as to completely fill all but one partition with live sectors.
    /// The addressed partitions are removed from the deadline, and new ones appended.
    /// The final partition in the deadline is always included in the compaction, whether or not explicitly requested.
    /// Removed sectors are removed from state entirely.
    /// May not be invoked if the deadline has any un-processed early terminations.
    fn compact_partitions<BS, RT>(
        rt: &mut RT,
        params: CompactPartitionsParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if params.deadline >= WPOST_PERIOD_DEADLINES {
            return Err(actor_error!(
                ErrIllegalArgument,
                "invalid deadline {}",
                params.deadline
            ));
        }

        let partition_count = params.partitions.len() as u64;

        rt.transaction(|state: &mut State, rt| {
            let info = get_miner_info(rt, state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            let store = rt.store();

            if !deadline_is_mutable(state.proving_period_start, params.deadline, rt.curr_epoch()) {
                return Err(actor_error!(
                    ErrForbidden,
                    "cannot compact deadline {} during its challenge window or the prior challenge window",
                    params.deadline
                ));
            }

            let submission_partition_limit =
                load_partitions_sectors_max(info.window_post_partition_sectors);
            if partition_count > submission_partition_limit {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "too many partitions {}, limit {}",
                    partition_count,
                    submission_partition_limit
                ));
            }

            let quant = state.quant_spec_for_deadline(params.deadline);
            let deadlines = state
                .load_deadlines(store)
                .map_err(|e| e.wrap("failed to load deadlines"))?;

            let mut deadline = deadlines
                .load_deadline(store, params.deadline)
                .map_err(|e| {
                    e.wrap(format!("failed to load deadline {}",
                    params.deadline))
                })?;

            let (live, dead, removed_power) = deadline
                .remove_partitions(store, &params.partitions, quant)
                .map_err(|e| {
                    ActorError::downcast(
                        e,
                        ExitCode::ErrIllegalState,
                        format!(
                            "failed to remove partitions from deadline {}",
                            params.deadline
                        ),
                    )
                })?;

            state.delete_sectors(store, &dead).map_err(|e| {
                actor_error!(ErrIllegalState, "failed to delete dead sectors: {:?}", e)
            })?;

            let sectors = state.load_sector_infos(store, &live).map_err(|e| {
                ActorError::downcast(e, ExitCode::ErrIllegalState, "failed to load moved sectors")
            })?;

            let new_power = deadline
                .add_sectors(
                    store,
                    info.window_post_partition_sectors,
                    &sectors,
                    info.sector_size,
                    quant,
                )
                .map_err(|e| {
                    ActorError::downcast(
                        e,
                        ExitCode::ErrIllegalState,
                        "failed to add back moved sectors",
                    )
                })?;

            if removed_power != new_power {
                return Err(actor_error!(
                    ErrIllegalState,
                    "power changed when compacting partitions: was {:?}, is now {:?}",
                    removed_power,
                    new_power
                ));
            }

            Ok(())
        })?;

        Ok(())
    }

    /// Compacts sector number allocations to reduce the size of the allocated sector
    /// number bitfield.
    ///
    /// When allocating sector numbers sequentially, or in sequential groups, this
    /// bitfield should remain fairly small. However, if the bitfield grows large
    /// enough such that PreCommitSector fails (or becomes expensive), this method
    /// can be called to mask out (throw away) entire ranges of unused sector IDs.
    /// For example, if sectors 1-99 and 101-200 have been allocated, sector number
    /// 99 can be masked out to collapse these two ranges into one.
    fn compact_sector_numbers<BS, RT>(
        rt: &mut RT,
        params: CompactSectorNumbersParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let last_sector_number = params
            .mask_sector_numbers
            .iter()
            .last()
            .ok_or_else(|| actor_error!(ErrIllegalArgument, "invalid mask bitfield"))?
            as SectorNumber;

        #[allow(clippy::absurd_extreme_comparisons)]
        if last_sector_number > MAX_SECTOR_NUMBER {
            return Err(actor_error!(
                ErrIllegalArgument,
                "masked sector number {} exceeded max sector number",
                last_sector_number
            ));
        }

        rt.transaction(|state: &mut State, rt| {
            let info = get_miner_info(rt, state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            state.mask_sector_number(rt.store(), &params.mask_sector_numbers)
        })?;

        Ok(())
    }

    /// Locks up some amount of a the miner's unlocked balance (including funds received alongside the invoking message).
    fn add_locked_fund<BS, RT>(rt: &mut RT, amount_to_lock: TokenAmount) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if amount_to_lock.is_negative() {
            return Err(actor_error!(
                ErrIllegalArgument,
                "cannot lock up a negative amount of funds"
            ));
        }

        let newly_vested = rt.transaction(|st: &mut State, rt| {
            let info = get_miner_info(rt, st)?;
            rt.validate_immediate_caller_is(info.control_addresses.iter().chain(&[
                info.worker,
                info.owner,
                *REWARD_ACTOR_ADDR,
            ]))?;

            // This may lock up unlocked balance that was covering InitialPledgeRequirements
            // This ensures that the amountToLock is always locked up if the miner account
            // can cover it.
            let unlocked_balance = st.get_unlocked_balance(&rt.current_balance()?);
            if unlocked_balance < amount_to_lock {
                return Err(actor_error!(
                    ErrInsufficientFunds,
                    "insufficient funds to lock, available: {}, requested: {}",
                    unlocked_balance,
                    amount_to_lock
                ));
            }

            let newly_vested = st
                .add_locked_funds(
                    rt.store(),
                    rt.curr_epoch(),
                    &amount_to_lock,
                    REWARD_VESTING_SPEC,
                )
                .map_err(|e| {
                    actor_error!(
                        ErrIllegalState,
                        "failed to lock funds in vesting table: {:?}",
                        e
                    )
                })?;

            Ok(newly_vested)
        })?;

        notify_pledge_changed(rt, &(amount_to_lock - newly_vested))?;
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
            .map_err(|e| actor_error!(ErrIllegalArgument, "fault not verified: {}", e))?
            .ok_or_else(|| actor_error!(ErrIllegalArgument, "Invalid fault"))?;

        // Elapsed since the fault (i.e. since the higher of the two blocks)
        let fault_age = rt.curr_epoch() - fault.epoch;
        if fault_age <= 0 {
            return Err(actor_error!(
                ErrIllegalArgument,
                "invalid fault epoch {} ahead of current {}",
                fault.epoch,
                rt.curr_epoch()
            ));
        }

        // Reward reporter with a share of the miner's current balance.
        let slasher_reward = reward_for_consensus_slash_report(fault_age, rt.current_balance()?);
        rt.send(reporter, METHOD_SEND, Default::default(), slasher_reward)?;

        let st: State = rt.state()?;

        rt.send(
            *STORAGE_POWER_ACTOR_ADDR,
            PowerMethod::OnConsensusFault as u64,
            Serialized::serialize(BigIntSer(&st.locked_funds))?,
            TokenAmount::zero(),
        )?;

        // close deals and burn funds
        terminate_miner(rt)?;

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
        if params.amount_requested.is_negative() {
            return Err(actor_error!(
                ErrIllegalArgument,
                "negative fund requested for withdrawal: {}",
                params.amount_requested
            ));
        }

        let (info, newly_vested) = rt.transaction(|state: &mut State, rt| {
            let info = get_miner_info(rt, state)?;

            // Only the owner is allowed to withdraw the balance as it belongs to/is controlled by the owner
            // and not the worker.
            rt.validate_immediate_caller_is(&[info.owner])?;

            // Ensure we don't have any pending terminations.
            if !state.early_terminations.is_empty() {
                return Err(actor_error!(
                    ErrForbidden,
                    "cannot withdraw funds while {} deadlines have terminated sectors with outstanding fees",
                    state.early_terminations.len()
                ));
            }

            // Unlock vested funds so we can spend them.
            let newly_vested = state
                .unlock_vested_funds(rt.store(), rt.curr_epoch())
                .map_err(|e| actor_error!(ErrIllegalState, "Failed to vest funds: {:?}", e))?;

            // Verify InitialPledgeRequirement does not exceed unlocked funds
            verify_pledge_meets_initial_requirements(rt, state)?;

            Ok((info, newly_vested))
        })?;

        let state: State = rt.state()?;

        let curr_balance = rt.current_balance()?;
        let amount_withdrawn = cmp::min(
            state.get_available_balance(&curr_balance),
            params.amount_requested,
        );
        assert!(!amount_withdrawn.is_negative());
        assert!(amount_withdrawn <= curr_balance);

        rt.send(
            info.owner,
            METHOD_SEND,
            Serialized::default(),
            amount_withdrawn,
        )?;

        notify_pledge_changed(rt, &newly_vested.neg())?;

        state.assert_balance_invariants(&rt.current_balance()?);
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
            CRON_EVENT_PROVING_DEADLINE => handle_proving_deadline(rt)?,
            CRON_EVENT_WORKER_KEY_CHANGE => commit_worker_key_change(rt)?,
            CRON_EVENT_PROCESS_EARLY_TERMINATIONS => {
                if process_early_terminations(rt)? {
                    schedule_early_termination_work(rt)?
                }
            }
            _ => {}
        };

        Ok(())
    }
}

/// Invoked at the end of each proving period, at the end of the epoch before the next one starts.
fn process_early_terminations<BS, RT>(rt: &mut RT) -> Result</* more */ bool, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let reward_stats = request_current_epoch_block_reward(rt)?;
    let power_total = request_current_total_power(rt)?;

    let (result, more, deals_to_terminate, penalty, pledge_delta) =
        rt.transaction(|state: &mut State, rt| {
            let store = rt.store();

            let (result, more) = state
                .pop_early_terminations(store, ADDRESSED_PARTITIONS_MAX, ADDRESSED_SECTORS_MAX)
                .map_err(|e| {
                    ActorError::downcast(
                        e,
                        ExitCode::ErrIllegalState,
                        "failed to pop early terminations",
                    )
                })?;

            // Nothing to do, don't waste any time.
            // This can happen if we end up processing early terminations
            // before the cron callback fires.
            if result.is_empty() {
                return Ok((
                    result,
                    more,
                    Vec::new(),
                    TokenAmount::zero(),
                    TokenAmount::zero(),
                ));
            }

            let info = get_miner_info(rt, state)?;
            let sectors = Sectors::load(store, &state.sectors).map_err(|e| {
                actor_error!(ErrIllegalState, "failed to load sectors array: {:?}", e)
            })?;

            let mut total_initial_pledge = TokenAmount::zero();
            let mut deals_to_terminate =
                Vec::<OnMinerSectorsTerminateParams>::with_capacity(result.sectors.len());
            let mut penalty = TokenAmount::zero();

            for (epoch, sector_numbers) in result.iter() {
                let sectors = sectors
                    .load_sector(sector_numbers)
                    .map_err(|e| e.wrap("failed to load sector infos"))?;

                penalty += termination_penalty(
                    info.sector_size,
                    epoch,
                    &reward_stats.this_epoch_reward_smoothed,
                    &power_total.quality_adj_power_smoothed,
                    &sectors,
                );

                // estimate ~one deal per sector.
                let mut deal_ids = Vec::<DealID>::with_capacity(sectors.len());
                for sector in sectors {
                    deal_ids.extend(sector.deal_ids);
                    total_initial_pledge += sector.initial_pledge;
                }

                let params = OnMinerSectorsTerminateParams { epoch, deal_ids };
                deals_to_terminate.push(params);
            }

            // Unlock funds for penalties.
            // We're intentionally reducing the penalty paid to what we have.
            let unlocked_balance = state.get_unlocked_balance(&rt.current_balance()?);
            let (penalty_from_vesting, penalty_from_balance) = state
                .penalize_funds_in_priority_order(
                    store,
                    rt.curr_epoch(),
                    &penalty,
                    &unlocked_balance,
                )
                .map_err(|e| {
                    ActorError::downcast(
                        e,
                        ExitCode::ErrIllegalState,
                        "failed to unlock unvested funds",
                    )
                })?;
            let penalty = &penalty_from_vesting + penalty_from_balance;

            // Remove pledge requirement.
            state.add_initial_pledge_requirement(&-&total_initial_pledge);
            let pledge_delta = -(total_initial_pledge + penalty_from_vesting);

            Ok((result, more, deals_to_terminate, penalty, pledge_delta))
        })?;

    // We didn't do anything, abort.
    if result.is_empty() {
        return Ok(more);
    }

    // Burn penalty.
    burn_funds(rt, penalty)?;

    // Return pledge.
    notify_pledge_changed(rt, &pledge_delta)?;

    // Terminate deals.
    for params in deals_to_terminate {
        request_terminate_deals(rt, params.epoch, params.deal_ids)?;
    }

    // reschedule cron worker, if necessary.
    Ok(more)
}

/// Invoked at the end of the last epoch for each proving deadline.
fn handle_proving_deadline<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let curr_epoch = rt.curr_epoch();

    let epoch_reward = request_current_epoch_block_reward(rt)?;
    let power_total = request_current_total_power(rt)?;

    let mut had_early_terminations = false;

    let mut power_delta = PowerPair::zero();
    let mut penalty_total = TokenAmount::zero();
    let mut pledge_delta = TokenAmount::zero();

    rt.transaction(|state: &mut State, rt| {
        // Vest locked funds.
        // This happens first so that any subsequent penalties are taken
        // from locked vesting funds before funds free this epoch.
        let newly_vested = state
            .unlock_vested_funds(rt.store(), rt.curr_epoch())
            .map_err(|e| {
                ActorError::downcast(e, ExitCode::ErrIllegalState, "failed to vest funds")
            })?;

        pledge_delta += -newly_vested;

        // expire pre-committed sectors
        let mut expiry_queue = BitFieldQueue::new(
            rt.store(),
            &state.pre_committed_sectors_expiry,
            state.quant_spec_every_deadline(),
        )
        .map_err(|e| {
            actor_error!(
                ErrIllegalState,
                "failed to load sector expiry queue: {:?}",
                e
            )
        })?;

        let (bitfield, modified) = expiry_queue.pop_until(curr_epoch).map_err(|e| {
            ActorError::downcast(
                e,
                ExitCode::ErrIllegalState,
                "failed to pop expired sectors",
            )
        })?;

        if modified {
            state.pre_committed_sectors_expiry = expiry_queue.amt.flush().map_err(|e| {
                actor_error!(ErrIllegalState, "failed to save expiry queue: {:?}", e)
            })?;
        }

        let deposit_to_burn = state
            .check_precommit_expiry(rt.store(), &bitfield)
            .map_err(|e| actor_error!(ErrIllegalState, "failed to save expiry queue: {:?}", e))?;

        penalty_total += deposit_to_burn;

        // Record whether or not we _had_ early terminations in the queue before this method.
        // That way, don't re-schedule a cron callback if one is already scheduled.
        had_early_terminations = have_pending_early_terminations(state);

        // Note: because the cron actor is not invoked on epochs with empty tipsets, the current epoch is not necessarily
        // exactly the final epoch of the deadline; it may be slightly later (i.e. in the subsequent deadline/period).
        // Further, this method is invoked once *before* the first proving period starts, after the actor is first
        // constructed; this is detected by !dlInfo.PeriodStarted().
        // Use dlInfo.PeriodEnd() rather than rt.CurrEpoch unless certain of the desired semantics.
        let deadline_info = state.deadline_info(curr_epoch);
        if !deadline_info.period_started() {
            // Skip checking faults on the first, incomplete period.
            return Ok(());
        }

        let mut deadlines = state
            .load_deadlines(rt.store())
            .map_err(|e| e.wrap("failed to load deadlines"))?;

        let mut deadline = deadlines
            .load_deadline(rt.store(), deadline_info.index)
            .map_err(|e| e.wrap(format!("failed to load deadline {}", deadline_info.index)))?;

        let quant = deadline_info.quant_spec();
        let mut unlocked_balance = state.get_unlocked_balance(&rt.current_balance()?);

        // Detect and penalize missing proofs.
        let fault_expiration = deadline_info.last() + FAULT_MAX_AGE;
        let mut penalize_power_total = TokenAmount::zero();

        let (new_faulty_power, failed_recovery_power) = deadline
            .process_deadline_end(rt.store(), quant, fault_expiration)
            .map_err(|e| {
                e.wrap(format!(
                    "failed to process end of deadline {}",
                    deadline_info.index
                ))
            })?;

        power_delta -= &new_faulty_power;
        penalize_power_total += new_faulty_power.qa;
        penalize_power_total += failed_recovery_power.qa;

        // Unlock sector penalty for all undeclared faults.
        let mut penalty_target = pledge_penalty_for_undeclared_fault(
            &epoch_reward.this_epoch_reward_smoothed,
            &power_total.quality_adj_power_smoothed,
            &penalize_power_total,
        );

        // Subtract the "ongoing" fault fee from the amount charged now, since it will be added on just below.
        penalty_target -= pledge_penalty_for_declared_fault(
            &epoch_reward.this_epoch_reward_smoothed,
            &power_total.quality_adj_power_smoothed,
            &penalize_power_total,
        );

        let (penalty_from_vesting, penalty_from_balance) = state
            .penalize_funds_in_priority_order(
                rt.store(),
                curr_epoch,
                &penalty_target,
                &unlocked_balance,
            )
            .map_err(|e| {
                ActorError::downcast(e, ExitCode::ErrIllegalState, "failed to unlock penalty")
            })?;

        unlocked_balance -= &penalty_from_balance;
        penalty_total += &penalty_from_vesting;
        penalty_total += penalty_from_balance;
        pledge_delta -= penalty_from_vesting;

        // Record faulty power for penalisation of ongoing faults, before popping expirations.
        // This includes any power that was just faulted from missing a PoSt.
        let penalty_target = pledge_penalty_for_declared_fault(
            &epoch_reward.this_epoch_reward_smoothed,
            &power_total.quality_adj_power_smoothed,
            &deadline.faulty_power.qa,
        );

        let (penalty_from_vesting, penalty_from_balance) = state
            .penalize_funds_in_priority_order(
                rt.store(),
                curr_epoch,
                &penalty_target,
                &unlocked_balance,
            )
            .map_err(|e| {
                ActorError::downcast(e, ExitCode::ErrIllegalState, "failed to unlock penalty")
            })?;

        unlocked_balance -= &penalty_from_balance;
        penalty_total += &penalty_from_vesting;
        penalty_total += penalty_from_balance;
        pledge_delta -= penalty_from_vesting;

        // Expire sectors that are due, either for on-time expiration or "early" faulty-for-too-long.
        let expired = deadline
            .pop_expired_sectors(rt.store(), deadline_info.last(), quant)
            .map_err(|e| {
                ActorError::downcast(
                    e,
                    ExitCode::ErrIllegalState,
                    "failed to load expired sectors",
                )
            })?;

        // Release pledge requirements for the sectors expiring on-time.
        // Pledge for the sectors expiring early is retained to support the termination fee that will be assessed
        // when the early termination is processed.
        pledge_delta -= &expired.on_time_pledge;
        state.add_initial_pledge_requirement(&-expired.on_time_pledge);

        // Record reduction in power of the amount of expiring active power.
        // Faulty power has already been lost, so the amount expiring can be excluded from the delta.
        power_delta -= &expired.active_power;

        // Record deadlines with early terminations. While this
        // bitfield is non-empty, the miner is locked until they
        // pay the fee.
        let no_early_terminations = expired.early_sectors.is_empty();
        if !no_early_terminations {
            state.early_terminations.set(deadline_info.index as usize);
        }

        // The termination fee is paid later, in early-termination queue processing.
        // We could charge at least the undeclared fault fee here, which is a lower bound on the penalty.
        // https://github.com/filecoin-project/specs-actors/issues/674

        // The deals are not terminated yet, that is left for processing of the early termination queue.

        // Save new deadline state.
        deadlines
            .update_deadline(rt.store(), deadline_info.index, &deadline)
            .map_err(|e| {
                ActorError::downcast(
                    e,
                    ExitCode::ErrIllegalState,
                    format!("failed to update deadline {}", deadline_info.index),
                )
            })?;

        state.save_deadlines(rt.store(), deadlines).map_err(|e| {
            ActorError::downcast(e, ExitCode::ErrIllegalState, "failed to save deadlines")
        })?;

        // Increment current deadline, and proving period if necessary.
        if deadline_info.period_started() {
            state.current_deadline += 1;
            state.current_deadline %= WPOST_PERIOD_DEADLINES;

            if state.current_deadline == 0 {
                state.proving_period_start += WPOST_PROVING_PERIOD;
            }
        }

        Ok(())
    })?;

    // Remove power for new faults, and burn penalties.
    request_update_power(rt, power_delta)?;
    burn_funds(rt, penalty_total)?;
    notify_pledge_changed(rt, &pledge_delta)?;

    // Schedule cron callback for next deadline's last epoch.
    let state: State = rt.state()?;
    let new_deadline_info = state.deadline_info(curr_epoch);
    enroll_cron_event(
        rt,
        new_deadline_info.last(),
        CronEventPayload {
            event_type: CRON_EVENT_PROVING_DEADLINE,
        },
    )?;

    // Record whether or not we _have_ early terminations now.
    let has_early_terminations = have_pending_early_terminations(&state);

    // If we didn't have pending early terminations before, but we do now,
    // handle them at the next epoch.
    if !had_early_terminations && has_early_terminations {
        // First, try to process some of these terminations.
        if process_early_terminations(rt)? {
            // If that doesn't work, just defer till the next epoch.
            schedule_early_termination_work(rt)?;
        }

        // Note: _don't_ process early terminations if we had a cron
        // callback already scheduled. In that case, we'll already have
        // processed AddressedSectorsMax terminations this epoch.
    }

    Ok(())
}

/// Check expiry is exactly *the epoch before* the start of a proving period.
fn validate_expiration<BS, RT>(
    rt: &RT,
    activation: ChainEpoch,
    expiration: ChainEpoch,
    seal_proof: RegisteredSealProof,
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    // expiration cannot be less than minimum after activation
    if expiration - activation < MIN_SECTOR_EXPIRATION {
        return Err(actor_error!(
            ErrIllegalArgument,
            "invalid expiration {}, total sector lifetime ({}) must exceed {} after activation {}",
            expiration,
            expiration - activation,
            MIN_SECTOR_EXPIRATION,
            activation
        ));
    }

    // expiration cannot exceed MaxSectorExpirationExtension from now
    if expiration > rt.curr_epoch() + MAX_SECTOR_EXPIRATION_EXTENSION {
        return Err(actor_error!(
            ErrIllegalArgument,
            "invalid expiration {}, cannot be more than {} past current epoch {}",
            expiration,
            MAX_SECTOR_EXPIRATION_EXTENSION,
            rt.curr_epoch()
        ));
    }

    // total sector lifetime cannot exceed SectorMaximumLifetime for the sector's seal proof
    if expiration - activation > seal_proof.sector_maximum_lifetime() {
        return Err(actor_error!(
            ErrIllegalArgument,
            "invalid expiration {}, total sector lifetime ({}) cannot exceed {} after activation {}",
            expiration,
            expiration - activation,
            seal_proof.sector_maximum_lifetime(),
            activation
        ));
    }

    Ok(())
}

fn validate_replace_sector<BS>(
    state: &State,
    store: &BS,
    params: &SectorPreCommitInfo,
) -> Result<SectorOnChainInfo, ActorError>
where
    BS: BlockStore,
{
    let replace_sector = state
        .get_sector(store, params.replace_sector_number)
        .map_err(|e| {
            actor_error!(
                ErrIllegalState,
                "failed to load sector {}: {}",
                params.sector_number,
                e
            )
        })?
        .ok_or_else(|| {
            actor_error!(
                ErrNotFound,
                "no such sector {} to replace",
                params.replace_sector_number
            )
        })?;

    if !replace_sector.deal_ids.is_empty() {
        return Err(actor_error!(
            ErrIllegalArgument,
            "cannot replace sector {} which has deals",
            params.replace_sector_number
        ));
    }

    if params.seal_proof != replace_sector.seal_proof {
        return Err(actor_error!(
            ErrIllegalArgument,
            "cannot replace sector {} seal proof {:?} with seal proof {:?}",
            params.replace_sector_number,
            replace_sector.seal_proof,
            params.seal_proof
        ));
    }

    if params.expiration < replace_sector.expiration {
        return Err(actor_error!(
            ErrIllegalArgument,
            "cannot replace sector {} expiration {} with sooner expiration {}",
            params.replace_sector_number,
            replace_sector.expiration,
            params.expiration
        ));
    }

    state
        .check_sector_health(
            store,
            params.replace_sector_deadline,
            params.replace_sector_partition,
            params.replace_sector_number,
        )
        .map_err(|e| {
            ActorError::downcast(
                e,
                ExitCode::ErrIllegalState,
                format!("failed to replace sector {}", params.replace_sector_number),
            )
        })?;

    Ok(replace_sector)
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
    let payload = Serialized::serialize(cb)
        .map_err(|e| actor_error!(ErrIllegalArgument, "failed to serialize payload: {}", e))?;

    let ser_params = Serialized::serialize(EnrollCronEventParams {
        event_epoch,
        payload,
    })?;
    rt.send(
        *STORAGE_POWER_ACTOR_ADDR,
        PowerMethod::EnrollCronEvent as u64,
        ser_params,
        TokenAmount::zero(),
    )?;

    Ok(())
}

fn request_update_power<BS, RT>(rt: &mut RT, delta: PowerPair) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if delta.is_zero() {
        return Ok(());
    }

    let delta_clone = delta.clone();

    rt.send(
        *STORAGE_POWER_ACTOR_ADDR,
        crate::power::Method::UpdateClaimedPower as MethodNum,
        Serialized::serialize(crate::power::UpdateClaimedPowerParams {
            raw_byte_delta: delta.raw,
            quality_adjusted_delta: delta.qa,
        })?,
        TokenAmount::zero(),
    )
    .map_err(|e| e.wrap(format!("failed to update power with {:?}", delta_clone)))?;

    Ok(())
}

fn request_terminate_deals<BS, RT>(
    rt: &mut RT,
    epoch: ChainEpoch,
    deal_ids: Vec<DealID>,
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    const MAX_LENGTH: usize = 8192;

    for chunk in deal_ids.chunks(MAX_LENGTH) {
        rt.send(
            *STORAGE_MARKET_ACTOR_ADDR,
            MarketMethod::OnMinerSectorsTerminate as u64,
            Serialized::serialize(OnMinerSectorsTerminateParamsRef {
                epoch,
                deal_ids: chunk,
            })?,
            TokenAmount::zero(),
        )?;
    }

    Ok(())
}

fn request_terminate_all_deals<BS, RT>(rt: &mut RT, state: &State) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let mut deal_ids = Vec::new();

    state
        .for_each_sector(rt.store(), |sector| {
            deal_ids.extend_from_slice(&sector.deal_ids);
            Ok(())
        })
        .map_err(|e| {
            actor_error!(
                ErrIllegalState,
                "failed to traverse sectors for termination: {:?}",
                e
            )
        })?;

    request_terminate_deals(rt, rt.curr_epoch(), deal_ids)
}

fn schedule_early_termination_work<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    enroll_cron_event(
        rt,
        rt.curr_epoch() + 1,
        CronEventPayload {
            event_type: CRON_EVENT_PROCESS_EARLY_TERMINATIONS,
        },
    )
}

fn have_pending_early_terminations(state: &State) -> bool {
    let no_early_terminations = state.early_terminations.is_empty();
    !no_early_terminations
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
        rt.get_randomness_from_beacon(WindowedPoStChallengeSeed, challenge_epoch, &entropy)?;

    let challenged_sectors = sectors
        .iter()
        .map(|s| SectorInfo {
            proof: s.seal_proof,
            sector_number: s.sector_number,
            sealed_cid: s.sealed_cid.clone(),
        })
        .collect();

    // get public inputs
    let pv_info = WindowPoStVerifyInfo {
        randomness,
        proofs,
        challenged_sectors,
        prover: miner_actor_id,
    };

    // verify the post proof
    rt.syscalls()
        .verify_post(&pv_info)
        .map_err(|e| actor_error!(ErrIllegalArgument, "invalid PoSt: {:?}, {}", pv_info, e))?;

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
        return Err(actor_error!(ErrForbidden, "too early to prove sector"));
    }

    // check randomness
    let challenge_earliest = seal_challenge_earliest(rt.curr_epoch(), params.registered_seal_proof);
    if params.seal_rand_epoch < challenge_earliest {
        return Err(actor_error!(
            ErrIllegalArgument,
            "seal epoch {} too old, expected >= {}",
            params.seal_rand_epoch,
            challenge_earliest
        ));
    }

    let commd = request_unsealed_sector_cid(rt, params.registered_seal_proof, &params.deal_ids)?;

    let miner_actor_id: u64 = if let Payload::ID(i) = rt.message().receiver().payload() {
        *i
    } else {
        panic!("could not provide ID address");
    };
    let entropy = rt.message().receiver().marshal_cbor().unwrap();
    let randomness: SealRandom =
        rt.get_randomness_from_tickets(SealRandomness, params.seal_rand_epoch, &entropy)?;
    let interactive_randomness: InteractiveSealRandomness = rt.get_randomness_from_beacon(
        InteractiveSealChallengeSeed,
        params.interactive_epoch,
        &entropy,
    )?;

    Ok(SealVerifyInfo {
        registered_proof: params.registered_seal_proof,
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

/// Closes down this miner by erasing its power, terminating all its deals and burning its funds.
fn terminate_miner<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let state: State = rt.state()?;
    request_terminate_all_deals(rt, &state)?;

    // Delete the actor and burn all remaining funds
    rt.delete_actor(&BURNT_FUNDS_ACTOR_ADDR)?;

    Ok(())
}

/// Requests the storage market actor compute the unsealed sector CID from a sector's deals.
fn request_unsealed_sector_cid<BS, RT>(
    rt: &mut RT,
    sector_type: RegisteredSealProof,
    deal_ids: &[DealID],
) -> Result<Cid, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let ret = rt.send(
        *STORAGE_MARKET_ACTOR_ADDR,
        MarketMethod::ComputeDataCommitment as u64,
        Serialized::serialize(ComputeDataCommitmentParamsRef {
            sector_type,
            deal_ids,
        })?,
        TokenAmount::zero(),
    )?;
    let unsealed_cid: Cid = ret.deserialize()?;
    Ok(unsealed_cid)
}

fn request_deal_weight<BS, RT>(
    rt: &mut RT,
    deal_ids: &[DealID],
    sector_start: ChainEpoch,
    sector_expiry: ChainEpoch,
) -> Result<VerifyDealsForActivationReturn, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let serialized = rt.send(
        *STORAGE_MARKET_ACTOR_ADDR,
        MarketMethod::VerifyDealsForActivation as u64,
        Serialized::serialize(VerifyDealsForActivationParamsRef {
            deal_ids,
            sector_start,
            sector_expiry,
        })?,
        TokenAmount::zero(),
    )?;

    Ok(serialized.deserialize()?)
}

fn commit_worker_key_change<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    rt.transaction(|state: &mut State, rt| {
        let mut info = get_miner_info(rt, state)?;

        // A previously scheduled key change could have been replaced with a new key change request
        // scheduled in the future. This case should be treated as a no-op.
        let key = match info.pending_worker_key {
            Some(key) if key.effective_at <= rt.curr_epoch() => key,
            _ => return Ok(()),
        };

        info.worker = key.new_worker;
        info.pending_worker_key = None;
        state.save_info(rt.store(), info).map_err(|e| {
            ActorError::downcast(e, ExitCode::ErrSerialization, "failed to save miner info")
        })?;

        Ok(())
    })
}

/// Requests the current epoch target block reward from the reward actor.
/// return value includes reward, smoothed estimate of reward, and baseline power
fn request_current_epoch_block_reward<BS, RT>(
    rt: &mut RT,
) -> Result<ThisEpochRewardReturn, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let ret = rt
        .send(
            *REWARD_ACTOR_ADDR,
            crate::reward::Method::ThisEpochReward as MethodNum,
            Default::default(),
            TokenAmount::zero(),
        )
        .map_err(|e| e.wrap("failed to check epoch baseline power"))?;

    let ret: ThisEpochRewardReturn = ret.deserialize().map_err(|e| {
        actor_error!(
            ErrSerialization,
            "failed to unmarshal target power value: {:?}",
            e
        )
    })?;

    Ok(ret)
}

/// Requests the current network total power and pledge from the power actor.
fn request_current_total_power<BS, RT>(rt: &mut RT) -> Result<CurrentTotalPowerReturn, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let ret = rt
        .send(
            *STORAGE_POWER_ACTOR_ADDR,
            crate::power::Method::CurrentTotalPower as MethodNum,
            Default::default(),
            TokenAmount::zero(),
        )
        .map_err(|e| e.wrap("failed to check current power"))?;

    let power: CurrentTotalPowerReturn = ret.deserialize().map_err(|e| {
        actor_error!(
            ErrSerialization,
            "failed to unmarshal power total value: {:?}",
            e
        )
    })?;

    Ok(power)
}

/// Verifies that the total locked balance exceeds the sum of sector initial pledges.
fn verify_pledge_meets_initial_requirements<BS, RT>(
    rt: &RT,
    state: &State,
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if state.meets_initial_pledge_condition(&rt.current_balance()?) {
        Ok(())
    } else {
        Err(actor_error!(
            ErrInsufficientFunds,
            "unlocked balance does not cover pledge requirements ({} < {})",
            state.get_unlocked_balance(&rt.current_balance()?),
            state.initial_pledge_requirement
        ))
    }
}

/// Resolves an address to an ID address and verifies that it is address of an account or multisig actor.
fn resolve_control_address<BS, RT>(rt: &RT, raw: Address) -> Result<Address, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let resolved = rt
        .resolve_address(&raw)?
        .ok_or_else(|| actor_error!(ErrIllegalArgument, "unable to resolve address: {}", raw))?;
    assert!(resolved.protocol() == Protocol::ID);

    let owner_code = rt
        .get_actor_code_cid(&resolved)?
        .ok_or_else(|| actor_error!(ErrIllegalArgument, "no code for address: {}", resolved))?;
    if !is_principal(&owner_code) {
        return Err(actor_error!(
            ErrIllegalArgument,
            "owner actor type must be a principal, was {}",
            owner_code
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
    let resolved = rt
        .resolve_address(&raw)?
        .ok_or_else(|| actor_error!(ErrIllegalArgument, "unable to resolve address: {}", raw))?;
    assert!(resolved.protocol() == Protocol::ID);

    let owner_code = rt
        .get_actor_code_cid(&resolved)?
        .ok_or_else(|| actor_error!(ErrIllegalArgument, "no code for address: {}", resolved))?;
    if owner_code != *ACCOUNT_ACTOR_CODE_ID {
        return Err(actor_error!(
            ErrIllegalArgument,
            "worker actor type must be an account, was {}",
            owner_code
        ));
    }

    if raw.protocol() != Protocol::BLS {
        let ret = rt.send(
            resolved,
            AccountMethod::PubkeyAddress as u64,
            Serialized::default(),
            TokenAmount::zero(),
        )?;
        let pub_key: Address = ret.deserialize().map_err(|e| {
            actor_error!(
                ErrSerialization,
                "failed to deserialize address result: {:?}, {}",
                ret,
                e
            )
        })?;
        if pub_key.protocol() != Protocol::BLS {
            return Err(actor_error!(
                ErrIllegalArgument,
                "worker account {} must have BLS pubkey, was {}",
                resolved,
                pub_key.protocol()
            ));
        }
    }
    Ok(resolved)
}

fn burn_funds<BS, RT>(rt: &mut RT, amount: TokenAmount) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if amount.is_positive() {
        rt.send(
            *BURNT_FUNDS_ACTOR_ADDR,
            METHOD_SEND,
            Serialized::default(),
            amount,
        )?;
    }
    Ok(())
}

fn notify_pledge_changed<BS, RT>(rt: &mut RT, pledge_delta: &BigInt) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    if !pledge_delta.is_zero() {
        rt.send(
            *STORAGE_POWER_ACTOR_ADDR,
            PowerMethod::UpdatePledgeTotal as u64,
            Serialized::serialize(BigIntSer(pledge_delta))?,
            TokenAmount::zero(),
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
    deadline_idx: u64,
    current_epoch: ChainEpoch,
) -> Result<DeadlineInfo, String> {
    if deadline_idx >= WPOST_PERIOD_DEADLINES {
        return Err(format!(
            "invalid deadline {}, must be < {}",
            deadline_idx, WPOST_PERIOD_DEADLINES
        ));
    }

    let deadline = DeadlineInfo::new(period_start, deadline_idx, current_epoch).next_not_elapsed();
    Ok(deadline)
}

/// Checks that a fault or recovery declaration at a specific deadline is outside the exclusion window for the deadline.
fn validate_fr_declaration_deadline(deadline: &DeadlineInfo) -> Result<(), String> {
    if deadline.fault_cutoff_passed() {
        Err("late fault or recovery declaration".to_string())
    } else {
        Ok(())
    }
}

/// Validates that a partition contains the given sectors.
fn validate_partition_contains_sectors(
    partition: &Partition,
    sectors: &BitField,
) -> Result<(), &'static str> {
    // Check that the declared sectors are actually assigned to the partition.
    if partition.sectors.contains_all(sectors) {
        Ok(())
    } else {
        Err("not all sectors are assigned to the partition")
    }
}

fn termination_penalty(
    sector_size: SectorSize,
    current_epoch: ChainEpoch,
    reward_estimate: &FilterEstimate,
    network_qa_power_estimate: &FilterEstimate,
    sectors: &[SectorOnChainInfo],
) -> TokenAmount {
    let mut total_fee = TokenAmount::zero();

    for sector in sectors {
        let sector_power = qa_power_for_sector(sector_size, sector);
        let fee = pledge_penalty_for_termination(
            &sector.expected_day_reward,
            &sector.expected_storage_pledge,
            current_epoch - sector.activation,
            reward_estimate,
            network_qa_power_estimate,
            &sector_power,
        );
        total_fee += fee;
    }

    total_fee
}

fn power_for_sector(sector_size: SectorSize, sector: &SectorOnChainInfo) -> PowerPair {
    PowerPair {
        raw: BigInt::from(sector_size as u64),
        qa: qa_power_for_sector(sector_size, sector),
    }
}

/// Returns the sum of the raw byte and quality-adjusted power for sectors.
fn power_for_sectors(sector_size: SectorSize, sectors: &[SectorOnChainInfo]) -> PowerPair {
    let qa = sectors
        .iter()
        .map(|s| qa_power_for_sector(sector_size, s))
        .sum();

    PowerPair {
        raw: BigInt::from(sector_size as u64) * BigInt::from(sectors.len()),
        qa,
    }
}

/// The oldest seal challenge epoch that will be accepted in the current epoch.
fn seal_challenge_earliest(current_epoch: ChainEpoch, proof: RegisteredSealProof) -> ChainEpoch {
    current_epoch - CHAIN_FINALITY - max_seal_duration(proof).unwrap_or_default()
}

fn get_miner_info<BS, RT>(rt: &RT, state: &State) -> Result<MinerInfo, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    state
        .get_info(rt.store())
        .map_err(|e| actor_error!(ErrIllegalState, "could not read miner info: {}", e))
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
                Self::change_peer_id(rt, params.deserialize()?)?;
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
                let ret = Self::terminate_sectors(rt, params.deserialize()?)?;
                Ok(Serialized::serialize(ret)?)
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
            Some(Method::CompactPartitions) => {
                Self::compact_partitions(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::CompactSectorNumbers) => {
                Self::compact_sector_numbers(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            None => Err(actor_error!(SysErrInvalidMethod, "Invalid method")),
        }
    }
}
