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
pub use sector_map::*;
pub use sectors::*;
pub use state::*;
pub use termination::*;
pub use types::*;
pub use vesting_state::*;

use crate::{
    account::Method as AccountMethod,
    actor_error,
    market::{self, ActivateDealsParams},
    power::MAX_MINER_PROVE_COMMITS_PER_EPOCH,
};
use crate::{
    check_empty_params, is_principal, smooth::FilterEstimate, ACCOUNT_ACTOR_CODE_ID,
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
    ActorDowncast,
};
use address::{Address, Payload, Protocol};
use bitfield::{UnvalidatedBitField, Validate};
use byteorder::{BigEndian, ByteOrder, WriteBytesExt};
use cid::{Cid, Code::Blake2b256, Prefix};
use clock::ChainEpoch;
use crypto::DomainSeparationTag::{
    self, InteractiveSealChallengeSeed, SealRandomness, WindowedPoStChallengeSeed,
};
use encoding::{BytesDe, Cbor};
use fil_types::{
    deadlines::DeadlineInfo, InteractiveSealRandomness, PoStProof, PoStRandomness,
    RegisteredSealProof, SealRandomness as SealRandom, SealVerifyInfo, SealVerifyParams, SectorID,
    SectorInfo, SectorNumber, SectorSize, WindowPoStVerifyInfo, MAX_SECTOR_NUMBER,
};
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser::BigIntSer;
use num_bigint::BigInt;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Signed, Zero};
use runtime::{ActorCode, Runtime};
use std::collections::{hash_map::Entry, HashMap};
use std::error::Error as StdError;
use std::{iter, ops::Neg};
use vm::{
    ActorError, DealID, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR,
    METHOD_SEND,
};

// The first 1000 actor-specific codes are left open for user error, i.e. things that might
// actually happen without programming error in the actor code.

// The following errors are particular cases of illegal state.
// They're not expected to ever happen, but if they do, distinguished codes can help us
// diagnose the problem.
use ExitCode::ErrPlaceholder as ErrBalanceInvariantBroken;

// * Updated to specs-actors commit: 17d3c602059e5c48407fb3c34343da87e6ea6586 (v0.9.12)

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
    ApplyRewards = 14,
    ReportConsensusFault = 15,
    WithdrawBalance = 16,
    ConfirmSectorProofsValid = 17,
    ChangeMultiaddrs = 18,
    CompactPartitions = 19,
    CompactSectorNumbers = 20,
    ConfirmUpdateWorkerKey = 21,
    RepayDebt = 22,
    ChangeOwnerAddress = 23,
    DisputeWindowedPoSt = 24,
}

/// Miner Actor
/// here in order to update the Power Actor to v3.
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

        check_control_addresses(&params.control_addresses)?;
        check_peer_info(&params.peer_id, &params.multi_addresses)?;

        let owner = resolve_control_address(rt, params.owner)?;
        let worker = resolve_worker_address(rt, params.worker)?;
        let control_addresses: Vec<_> = params
            .control_addresses
            .into_iter()
            .map(|address| resolve_control_address(rt, address))
            .collect::<Result<_, _>>()?;

        let current_epoch = rt.curr_epoch();
        let blake2b = |b: &[u8]| rt.hash_blake2b(b);
        let offset = assign_proving_period_offset(*rt.message().receiver(), current_epoch, blake2b)
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrSerialization,
                    "failed to assign proving period offset",
                )
            })?;

        let period_start = current_proving_period_start(current_epoch, offset);
        if period_start > current_epoch {
            return Err(actor_error!(
                ErrIllegalState,
                "computed proving period start {} after current epoch {}",
                period_start,
                current_epoch
            ));
        }

        let deadline_idx = current_deadline_index(current_epoch, period_start);
        if deadline_idx >= WPOST_PERIOD_DEADLINES as usize {
            return Err(actor_error!(
                ErrIllegalState,
                "computed proving deadline index {} invalid",
                deadline_idx
            ));
        }

        let info = MinerInfo::new(
            owner,
            worker,
            control_addresses,
            params.peer_id,
            params.multi_addresses,
            params.window_post_proof_type,
        )
        .map_err(|e| {
            actor_error!(
                ErrIllegalState,
                "failed to construct initial miner info: {}",
                e
            )
        })?;
        let info_cid = rt.store().put(&info, Blake2b256).map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                "failed to construct illegal state",
            )
        })?;

        let st = State::new(rt.store(), info_cid, period_start, deadline_idx).map_err(|e| {
            e.downcast_default(ExitCode::ErrIllegalState, "failed to construct state")
        })?;
        rt.create(&st)?;

        // Register first cron callback for epoch before the next deadline starts.
        let deadline_close =
            period_start + WPOST_CHALLENGE_WINDOW * (1 + deadline_idx) as ChainEpoch;
        enroll_cron_event(
            rt,
            deadline_close - 1,
            CronEventPayload {
                event_type: CRON_EVENT_PROVING_DEADLINE,
            },
        )?;

        Ok(())
    }

    fn control_addresses<BS, RT>(rt: &mut RT) -> Result<GetControlAddressesReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        let state: State = rt.state()?;
        let info = get_miner_info(rt.store(), &state)?;
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
        check_control_addresses(&params.new_control_addresses)?;

        let new_worker = resolve_worker_address(rt, params.new_worker)?;
        let control_addresses: Vec<Address> = params
            .new_control_addresses
            .into_iter()
            .map(|address| resolve_control_address(rt, address))
            .collect::<Result<_, _>>()?;

        rt.transaction(|state: &mut State, rt| {
            let mut info = get_miner_info(rt.store(), state)?;

            // Only the Owner is allowed to change the new_worker and control addresses.
            rt.validate_immediate_caller_is(std::iter::once(&info.owner))?;

            // save the new control addresses
            info.control_addresses = control_addresses;

            // save new_worker addr key change request
            if new_worker != info.worker && info.pending_worker_key.is_none() {
                info.pending_worker_key = Some(WorkerKeyChange {
                    new_worker,
                    effective_at: rt.curr_epoch() + WORKER_KEY_CHANGE_DELAY,
                })
            }

            state.save_info(rt.store(), &info).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "could not save miner info")
            })?;

            Ok(())
        })?;

        Ok(())
    }

    /// Triggers a worker address change if a change has been requested and its effective epoch has arrived.
    fn confirm_update_worker_key<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.transaction(|state: &mut State, rt| {
            let mut info = get_miner_info(rt.store(), &state)?;

            rt.validate_immediate_caller_is(std::iter::once(&info.owner))?;

            process_pending_worker(&mut info, rt, state)?;

            Ok(())
        })
    }

    /// Proposes or confirms a change of owner address.
    /// If invoked by the current owner, proposes a new owner address for confirmation. If the proposed address is the
    /// current owner address, revokes any existing proposal.
    /// If invoked by the previously proposed address, with the same proposal, changes the current owner address to be
    /// that proposed address.
    fn change_owner_address<BS, RT>(rt: &mut RT, new_address: Address) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // * Cannot match go checking for undef address, does go impl allow this to be
        // * deserialized over the wire? If so, a workaround will be needed

        if !matches!(new_address.protocol(), Protocol::ID) {
            return Err(actor_error!(
                ErrIllegalArgument,
                "owner address must be an ID address"
            ));
        }

        rt.transaction(|state: &mut State, rt| {
            let mut info = get_miner_info(rt.store(), &state)?;

            if rt.message().caller() == &info.owner || info.pending_owner_address.is_none() {
                rt.validate_immediate_caller_is(std::iter::once(&info.owner))?;
                info.pending_owner_address = Some(new_address);
            } else {
                let pending_address = info.pending_owner_address.unwrap();
                rt.validate_immediate_caller_is(std::iter::once(&pending_address))?;
                if new_address != pending_address {
                    return Err(actor_error!(
                        ErrIllegalArgument,
                        "expected confirmation of {} got {}",
                        pending_address,
                        new_address
                    ));
                }
                info.owner = pending_address;
            }

            // Clear ay no-op change
            if let Some(p_addr) = info.pending_owner_address {
                if p_addr == info.owner {
                    info.pending_owner_address = None;
                }
            }

            state.save_info(rt.store(), &info).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to save miner info")
            })?;

            Ok(())
        })
    }

    fn change_peer_id<BS, RT>(rt: &mut RT, params: ChangePeerIDParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        check_peer_info(&params.new_id, &[])?;

        rt.transaction(|state: &mut State, rt| {
            let mut info = get_miner_info(rt.store(), state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            info.peer_id = params.new_id;
            state.save_info(rt.store(), &info).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "could not save miner info")
            })?;

            Ok(())
        })?;
        Ok(())
    }

    fn change_multiaddresses<BS, RT>(
        rt: &mut RT,
        params: ChangeMultiaddrsParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        check_peer_info(&[], &params.new_multi_addrs)?;

        rt.transaction(|state: &mut State, rt| {
            let mut info = get_miner_info(rt.store(), state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            info.multi_address = params.new_multi_addrs;
            state.save_info(rt.store(), &info).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "could not save miner info")
            })?;

            Ok(())
        })?;
        Ok(())
    }

    /// Invoked by miner's worker address to submit their fallback post
    fn submit_windowed_post<BS, RT>(
        rt: &mut RT,
        mut params: SubmitWindowedPoStParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let current_epoch = rt.curr_epoch();

        if params.deadline >= WPOST_PERIOD_DEADLINES as usize {
            return Err(actor_error!(
                ErrIllegalArgument,
                "invalid deadline {} of {}",
                params.deadline,
                WPOST_PERIOD_DEADLINES
            ));
        }

        // * This check is invalid because our randomness length is always == 32
        // * and there is no clear need for less randomness
        // if params.chain_commit_rand.0.len() > RANDOMNESS_LENGTH {
        //     return Err(actor_error!(
        //         ErrIllegalArgument,
        //         "expected at most {} bytes of randomness, got {}",
        //         RANDOMNESS_LENGTH,
        //         params.chain_commit_rand.0.len()
        //     ));
        // }

        let post_result = rt.transaction(|state: &mut State, rt| {
            let info = get_miner_info(rt.store(), state)?;

            let max_proof_size = info.window_post_proof_type.proof_size().map_err(|e| {
                actor_error!(
                    ErrIllegalState,
                    "failed to determine max window post proof size: {}",
                    e
                )
            })?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            // Verify that the miner has passed 0 or 1 proofs. If they've
            // passed 1, verify that it's a good proof.
            //
            // This can be 0 if the miner isn't actually proving anything,
            // just skipping all sectors.
            if let Some(proof) = params.proofs.get(0) {
                if proof.post_proof != info.window_post_proof_type {
                    return Err(actor_error!(
                        ErrIllegalArgument,
                        "expected proof of type {:?}, got {:?}",
                        proof.post_proof,
                        info.window_post_proof_type
                    ));
                }
            } else {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "expected exactly one proof, got {}",
                    params.proofs.len()
                ));
            }
            // Make sure the proof size doesn't exceed the max. We could probably check for an exact match, but this is safer.
            let max_size = max_proof_size * params.partitions.len();
            if params.proofs.get(0).unwrap().proof_bytes.len() > max_size {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "expect proof to be smaller than {} bytes",
                    max_size
                ));
            }

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

            let current_deadline = state.deadline_info(current_epoch);

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
            if params.deadline != current_deadline.index as usize {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "invalid deadline {} at epoch {}, expected {}",
                    params.deadline,
                    current_epoch,
                    current_deadline.index
                ));
            }

            // Verify that the PoSt was committed to the chain at most
            // WPoStChallengeLookback+WPoStChallengeWindow in the past.
            if params.chain_commit_epoch < current_deadline.challenge {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "expected chain commit epoch {} to be after {}",
                    params.chain_commit_epoch,
                    current_deadline.challenge
                ));
            }

            if params.chain_commit_epoch >= current_epoch {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "chain commit epoch {} must be less tha the current epoch {}",
                    params.chain_commit_epoch,
                    current_epoch
                ));
            }

            // Verify the chain commit randomness
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

            let sectors = Sectors::load(rt.store(), &state.sectors).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to load sectors")
            })?;

            let mut deadlines = state
                .load_deadlines(rt.store())
                .map_err(|e| e.wrap("failed to load deadlines"))?;

            let mut deadline = deadlines
                .load_deadline(rt.store(), params.deadline)
                .map_err(|e| e.wrap(format!("failed to load deadline {}", params.deadline)))?;

            // Record proven sectors/partitions, returning updates to power and the final set of sectors
            // proven/skipped.
            //
            // NOTE: This function does not actually check the proofs but does assume that they're correct. Instead,
            // it snapshots the deadline's state and the submitted proofs at the end of the challenge window and
            // allows third-parties to dispute these proofs.
            //
            // While we could perform _all_ operations at the end of challenge window, we do as we can here to avoid
            // overloading cron.
            let fault_expiration = current_deadline.last() + FAULT_MAX_AGE;
            let post_result = deadline
                .record_proven_sectors(
                    rt.store(),
                    &sectors,
                    info.sector_size,
                    current_deadline.quant_spec(),
                    fault_expiration,
                    &mut params.partitions,
                )
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!(
                            "failed to process post submission for deadline {}",
                            params.deadline
                        ),
                    )
                })?;

            // Make sure we actually proved something.
            let proven_sectors = &post_result.sectors - &post_result.ignored_sectors;
            if proven_sectors.is_empty() {
                // Abort verification if all sectors are (now) faults. There's nothing to prove.
                // It's not rational for a miner to submit a Window PoSt marking *all* non-faulty sectors as skipped,
                // since that will just cause them to pay a penalty at deadline end that would otherwise be zero
                // if they had *not* declared them.
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "cannot prove partitions with no active sectors"
                ));
            }

            // If we're not recovering power, record the proof for optimistic verification.
            if post_result.recovered_power.is_zero() {
                deadline
                    .record_post_proofs(rt.store(), &post_result.partitions, &params.proofs)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            "failed to record proof for optimistic verification",
                        )
                    })?
            } else {
                // Load sector infos for proof, substituting a known-good sector for known-faulty sectors.
                // Note: this is slightly sub-optimal, loading info for the recovering sectors again after they were already
                // loaded above.
                let sector_infos = sectors
                    .load_for_proof(&post_result.sectors, &post_result.ignored_sectors)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            "failed to load sectors for post verification",
                        )
                    })?;
                verify_windowed_post(rt, current_deadline.challenge, &sector_infos, params.proofs)
                    .map_err(|e| e.wrap("window post failed"))?;
            }

            let deadline_idx = params.deadline;
            deadlines
                .update_deadline(rt.store(), params.deadline, &deadline)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to update deadline {}", deadline_idx),
                    )
                })?;

            state.save_deadlines(rt.store(), deadlines).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to save deadlines")
            })?;

            Ok(post_result)
        })?;

        // Restore power for recovered sectors. Remove power for new faults.
        // NOTE: It would be permissible to delay the power loss until the deadline closes, but that would require
        // additional accounting state.
        // https://github.com/filecoin-project/specs-actors/issues/414
        request_update_power(rt, post_result.power_delta)?;

        let state: State = rt.state()?;
        state
            .check_balance_invariants(&rt.current_balance()?)
            .map_err(|e| {
                ActorError::new(
                    ErrBalanceInvariantBroken,
                    format!("balance invariants broken: {}", e),
                )
            })?;

        Ok(())
    }

    fn dispute_windowed_post<BS, RT>(
        rt: &mut RT,
        params: DisputeWindowedPoStParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;
        let reporter = *rt.message().caller();

        if params.deadline >= WPOST_PERIOD_DEADLINES as usize {
            return Err(actor_error!(
                ErrIllegalArgument,
                "invalid deadline {} of {}",
                params.deadline,
                WPOST_PERIOD_DEADLINES
            ));
        }
        let current_epoch = rt.curr_epoch();

        // Note: these are going to be slightly inaccurate as time
        // will have moved on from when the post was actually
        // submitted.
        //
        // However, these are estimates _anyways_.
        let epoch_reward = request_current_epoch_block_reward(rt)?;
        let power_total = request_current_total_power(rt)?;

        let (pledge_delta, mut to_burn, power_delta, to_reward) =
            rt.transaction(|st: &mut State, rt| {
                if !deadline_available_for_optimistic_post_dispute(
                    st.proving_period_start,
                    params.deadline,
                    current_epoch,
                ) {
                    return Err(actor_error!(
                        ErrForbidden,
                        "can only dispute window posts during the dispute window\
                    ({} epochs after the challenge window closes)",
                        WPOST_DISPUTE_WINDOW
                    ));
                }

                let info = get_miner_info(rt.store(), st)?;
                // --- check proof ---

                // Find the proving period start for the deadline in question.
                let mut pp_start = st.proving_period_start;
                if st.current_deadline < params.deadline {
                    pp_start -= WPOST_PROVING_PERIOD
                }
                let target_deadline = new_deadline_info(pp_start, params.deadline, current_epoch);
                // Load the target deadline
                let mut deadlines_current = st
                    .load_deadlines(rt.store())
                    .map_err(|e| e.wrap("failed to load deadlines"))?;

                let mut dl_current = deadlines_current
                    .load_deadline(rt.store(), params.deadline)
                    .map_err(|e| e.wrap("failed to load deadline"))?;

                // Take the post from the snapshot for dispute.
                // This operation REMOVES the PoSt from the snapshot so
                // it can't be disputed again. If this method fails,
                // this operation must be rolled back.
                let (partitions, proofs) = dl_current
                    .take_post_proofs(rt.store(), params.post_index)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            "failed to load proof for dispute",
                        )
                    })?;

                // Load the partition info we need for the dispute.
                let mut dispute_info = dl_current
                    .load_partitions_for_dispute(rt.store(), partitions)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            "failed to load partition for dispute",
                        )
                    })?;

                // This includes power that is no longer active (e.g., due to sector terminations).
                // It must only be used for penalty calculations, not power adjustments.
                let penalised_power = dispute_info.disputed_power.clone();

                // Load sectors for the dispute.
                let sectors = Sectors::load(rt.store(), &st.sectors).map_err(|e| {
                    e.downcast_default(ExitCode::ErrIllegalState, "failed to load sectors array")
                })?;
                let sector_infos = sectors
                    .load_for_proof(
                        &dispute_info.all_sector_nos,
                        &dispute_info.ignored_sector_nos,
                    )
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            "failed to load sectors to dispute window post",
                        )
                    })?;

                // Check proof, we fail if validation succeeds.
                match verify_windowed_post(rt, target_deadline.challenge, &sector_infos, proofs) {
                    Ok(()) => {
                        return Err(actor_error!(
                            ErrIllegalArgument,
                            "failed to dispute valid post"
                        ));
                    }
                    Err(e) => {
                        log::info!("Successfully disputed: {}", e);
                    }
                }

                // Ok, now we record faults. This always works because
                // we don't allow compaction/moving sectors during the
                // challenge window.
                //
                // However, some of these sectors may have been
                // terminated. That's fine, we'll skip them.
                let fault_expiration_epoch = target_deadline.last() + FAULT_MAX_AGE;
                let power_delta = dl_current
                    .record_faults(
                        rt.store(),
                        &sectors,
                        info.sector_size,
                        quant_spec_for_deadline(&target_deadline),
                        fault_expiration_epoch,
                        &mut dispute_info.disputed_sectors,
                    )
                    .map_err(|e| {
                        e.downcast_default(ExitCode::ErrIllegalState, "failed to declare faults")
                    })?;

                deadlines_current
                    .update_deadline(rt.store(), params.deadline, &dl_current)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            format!("failed to update deadline {}", params.deadline),
                        )
                    })?;

                st.save_deadlines(rt.store(), deadlines_current)
                    .map_err(|e| {
                        e.downcast_default(ExitCode::ErrIllegalState, "failed to save deadlines")
                    })?;

                // --- penalties ---

                // Calculate the base penalty.
                let penalty_base = pledge_penalty_for_invalid_windowpost(
                    &epoch_reward.this_epoch_reward_smoothed,
                    &power_total.quality_adj_power_smoothed,
                    &penalised_power.qa,
                );

                // Calculate the target reward.
                let reward_target =
                    reward_for_disputed_window_post(info.window_post_proof_type, penalised_power);

                // Compute the target penalty by adding the
                // base penalty to the target reward. We don't
                // take reward out of the penalty as the miner
                // could end up receiving a substantial
                // portion of their fee back as a reward.
                let penalty_target = &penalty_base + &reward_target;
                st.apply_penalty(&penalty_target)
                    .map_err(|e| actor_error!(ErrIllegalState, "failed to apply penalty {}", e))?;
                let (penalty_from_vesting, penalty_from_balance) = st
                    .repay_partial_debt_in_priority_order(
                        rt.store(),
                        current_epoch,
                        &rt.current_balance()?,
                    )
                    .map_err(|e| {
                        e.downcast_default(ExitCode::ErrIllegalState, "failed to pay debt")
                    })?;

                let to_burn = &penalty_from_vesting + &penalty_from_balance;

                // Now, move as much of the target reward as
                // we can from the burn to the reward.
                let to_reward = std::cmp::min(&to_burn, &reward_target);
                let to_burn = &to_burn - to_reward;
                let pledge_delta = penalty_from_vesting.neg();

                Ok((pledge_delta, to_burn, power_delta, to_reward.clone()))
            })?;

        request_update_power(rt, power_delta)?;
        if !to_reward.is_zero() {
            if let Err(e) = rt.send(
                reporter,
                METHOD_SEND,
                Serialized::default(),
                to_reward.clone(),
            ) {
                log::error!("failed to send reward: {}", e);
                to_burn += to_reward;
            }
        }

        burn_funds(rt, to_burn)?;
        notify_pledge_changed(rt, &pledge_delta)?;

        let st: State = rt.state()?;
        st.check_balance_invariants(&rt.current_balance()?)
            .map_err(|e| {
                ActorError::new(
                    ErrBalanceInvariantBroken,
                    format!("balance invariants broken: {}", e),
                )
            })?;
        Ok(())
    }

    /// Proposals must be posted on chain via sma.PublishStorageDeals before PreCommitSector.
    /// Optimization: PreCommitSector could contain a list of deals that are not published yet.
    fn pre_commit_sector<BS, RT>(
        rt: &mut RT,
        params: PreCommitSectorParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if !can_pre_commit_seal_proof(params.seal_proof, rt.network_version()) {
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

        if Prefix::from(params.sealed_cid) != SEALED_CID_PREFIX {
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

        let challenge_earliest = rt.curr_epoch() - MAX_PRE_COMMIT_RANDOMNESS_LOOKBACK;
        if params.seal_rand_epoch < challenge_earliest {
            return Err(actor_error!(
                ErrIllegalArgument,
                "seal challenge epoch {} too old, must be after {}",
                params.seal_rand_epoch,
                challenge_earliest
            ));
        }

        // Require sector lifetime meets minimum by assuming activation happens at last epoch permitted for seal proof.
        // This could make sector maximum lifetime validation more lenient if the maximum sector limit isn't hit first.
        let max_activation =
            rt.curr_epoch() + max_prove_commit_duration(params.seal_proof).unwrap_or_default();
        validate_expiration(rt, max_activation, params.expiration, params.seal_proof)?;

        if params.replace_capacity && params.deal_ids.is_empty() {
            return Err(actor_error!(
                ErrIllegalArgument,
                "cannot replace sector without committing deals"
            ));
        }

        if params.replace_sector_deadline >= WPOST_PERIOD_DEADLINES as usize {
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
        let deal_weights = request_deal_weights(
            rt,
            &[market::SectorDeals {
                sector_expiry: params.expiration,
                deal_ids: params.deal_ids.clone(),
            }],
        )?;
        let deal_weight = &deal_weights.sectors[0];
        let mut fee_to_burn = TokenAmount::from(0);
        let newly_vested = rt.transaction(|state: &mut State, rt| {
            let newly_vested = TokenAmount::from(0);

            // available balance already accounts for fee debt so it is correct to call
            // this before RepayDebts. We would have to
            // subtract fee debt explicitly if we called this after.
            let available_balance = state
                .get_available_balance(&rt.current_balance()?)
                .map_err(|e| {
                    actor_error!(
                        ErrIllegalState,
                        "failed to calculate available balance: {}",
                        e
                    )
                })?;
            fee_to_burn = repay_debts_or_abort(rt, state)?;

            let info = get_miner_info(rt.store(), state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            if consensus_fault_active(&info, rt.curr_epoch()) {
                return Err(actor_error!(
                    ErrForbidden,
                    "precommit not allowed during active consensus fault"
                ));
            }

            // From network version 7, the pre-commit seal type must have the same Window PoSt proof type as the miner's
            // recorded seal type has, rather than be exactly the same seal type.
            // This permits a transition window from V1 to V1_1 seal types (which share Window PoSt proof type).
            let sector_wpost_proof =
                params
                    .seal_proof
                    .registered_window_post_proof()
                    .map_err(|e| {
                        actor_error!(
                            ErrIllegalState,
                            "failed to lookup window PoSt proof type \
                            for sector seal proof {:?}: {}",
                            params.seal_proof,
                            e
                        )
                    })?;
            if sector_wpost_proof != info.window_post_proof_type {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "sector window PoSt proof type {:?} must match miner \
                        window PoSt proof type {:?}",
                    sector_wpost_proof,
                    info.window_post_proof_type
                ));
            }

            let store = rt.store();

            let deal_count_max = sector_deals_max(info.sector_size);
            if params.deal_ids.len() as u64 > deal_count_max {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "too many deals for sector {} > {}",
                    params.deal_ids.len(),
                    deal_count_max
                ));
            }

            // Ensure total deal space does not exceed sector size.
            if deal_weight.deal_space > info.sector_size as u64 {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "deal size too large to fit in sector {} > {}",
                    deal_weight.deal_space,
                    info.sector_size
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

            // This sector check is redundant given the allocated sectors bitfield, but remains for safety.
            let sector_found = state
                .has_sector_number(store, params.sector_number)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to check sector {}", params.sector_number),
                    )
                })?;

            if sector_found {
                return Err(actor_error!(
                    ErrIllegalState,
                    "sector {} already committed",
                    params.sector_number
                ));
            }

            if params.replace_capacity {
                validate_replace_sector(state, store, &params)?;
            }

            let duration = params.expiration - rt.curr_epoch();

            let sector_weight = qa_power_for_weight(
                info.sector_size,
                duration,
                &deal_weight.deal_weight,
                &deal_weight.verified_deal_weight,
            );

            let deposit_req = pre_commit_deposit_for_power(
                &reward_stats.this_epoch_reward_smoothed,
                &power_total.quality_adj_power_smoothed,
                &sector_weight,
            );

            if available_balance < deposit_req {
                return Err(actor_error!(
                    ErrInsufficientFunds,
                    "insufficient funds for pre-commit deposit: {}",
                    deposit_req
                ));
            }

            state.add_pre_commit_deposit(&deposit_req).map_err(|e| {
                actor_error!(
                    ErrIllegalState,
                    "failed to add pre-commit deposit {}: {}",
                    deposit_req,
                    e
                )
            })?;

            let seal_proof = params.seal_proof;
            let sector_number = params.sector_number;

            state
                .put_precommitted_sector(
                    store,
                    SectorPreCommitOnChainInfo {
                        info: params,
                        pre_commit_deposit: deposit_req,
                        pre_commit_epoch: rt.curr_epoch(),
                        deal_weight: deal_weight.deal_weight.clone(),
                        verified_deal_weight: deal_weight.verified_deal_weight.clone(),
                    },
                )
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to write pre-committed sector {}", sector_number),
                    )
                })?;

            // add precommit expiry to the queue
            let max_seal_duration = max_prove_commit_duration(seal_proof).ok_or_else(|| {
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
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        "failed to add pre-commit expiry to queue",
                    )
                })?;

            Ok(newly_vested)
        })?;

        burn_funds(rt, fee_to_burn)?;
        let state: State = rt.state()?;
        state
            .check_balance_invariants(&rt.current_balance()?)
            .map_err(|e| {
                ActorError::new(
                    ErrBalanceInvariantBroken,
                    format!("balance invariants broken: {}", e),
                )
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

        if params.sector_number > MAX_SECTOR_NUMBER {
            return Err(actor_error!(
                ErrIllegalArgument,
                "sector number greater than maximum"
            ));
        }

        let sector_number = params.sector_number;

        let st: State = rt.state()?;
        let precommit = st
            .get_precommitted_sector(rt.store(), sector_number)
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    format!("failed to load pre-committed sector {}", sector_number),
                )
            })?
            .ok_or_else(|| actor_error!(ErrNotFound, "no pre-commited sector {}", sector_number))?;

        let max_proof_size = precommit.info.seal_proof.proof_size().map_err(|e| {
            actor_error!(
                ErrIllegalState,
                "failed to determine max proof size for sector {}: {}",
                sector_number,
                e
            )
        })?;
        if params.proof.len() > max_proof_size {
            return Err(actor_error!(
                ErrIllegalArgument,
                "sector prove-commit proof of size {} exceeds max size of {}",
                params.proof.len(),
                max_proof_size
            ));
        }

        let msd = max_prove_commit_duration(precommit.info.seal_proof).ok_or_else(|| {
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
                sealed_cid: precommit.info.sealed_cid,
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

        // This should be enforced by the power actor. We log here just in case
        // something goes wrong.
        if params.sectors.len() > MAX_MINER_PROVE_COMMITS_PER_EPOCH {
            log::warn!(
                "confirmed more prove commits in an epoch than permitted: {} > {}",
                params.sectors.len(),
                MAX_MINER_PROVE_COMMITS_PER_EPOCH
            );
        }

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
        let info = get_miner_info(rt.store(), &state)?;

        //
        // Activate storage deals.
        //

        // This skips missing pre-commits.
        let precommitted_sectors = state
            .find_precommitted_sectors(rt.store(), &params.sectors)
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to load pre-committed sectors",
                )
            })?;

        // Committed-capacity sectors licensed for early removal by new sectors being proven.
        let mut replace_sectors = DeadlineSectorMap::new();

        // Pre-commits for new sectors.
        let mut pre_commits = Vec::<SectorPreCommitOnChainInfo>::new();

        for pre_commit in precommitted_sectors {
            if !pre_commit.info.deal_ids.is_empty() {
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

        let (total_pledge, newly_vested) = rt.transaction(|state: &mut State, rt| {
            let store = rt.store();

            // Schedule expiration for replaced sectors to the end of their next deadline window.
            // They can't be removed right now because we want to challenge them immediately before termination.
            let replaced = state
                .reschedule_sector_expirations(
                    store,
                    rt.curr_epoch(),
                    info.sector_size,
                    replace_sectors,
                )
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        "failed to replace sector expirations",
                    )
                })?;

            let replaced_by_sector_number: HashMap<u64, SectorOnChainInfo> =
                replaced.into_iter().map(|s| (s.sector_number, s)).collect();

            let mut new_sector_numbers = Vec::<SectorNumber>::with_capacity(pre_commits.len());
            let mut deposit_to_unlock = TokenAmount::zero();
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
                // It's not capped to 1 FIL, so can exceed the actual initial pledge requirement.
                let storage_pledge = expected_reward_for_power(
                    &reward_stats.this_epoch_reward_smoothed,
                    &power_total.quality_adj_power_smoothed,
                    &power,
                    INITIAL_PLEDGE_PROJECTION_PERIOD,
                );

                let mut initial_pledge = initial_pledge_for_power(
                    &power,
                    &reward_stats.this_epoch_baseline_power,
                    &reward_stats.this_epoch_reward_smoothed,
                    &power_total.quality_adj_power_smoothed,
                    &circulating_supply,
                );

                // Lower-bound the pledge by that of the sector being replaced.
                // Record the replaced age and reward rate for termination fee calculations.
                let (replaced_pledge, replaced_sector_age, replaced_day_reward) =
                    replaced_sector_parameters(
                        rt.curr_epoch(),
                        &pre_commit,
                        &replaced_by_sector_number,
                    )?;
                initial_pledge = std::cmp::max(initial_pledge, replaced_pledge);

                deposit_to_unlock += &pre_commit.pre_commit_deposit;
                total_pledge += &initial_pledge;

                let new_sector_info = SectorOnChainInfo {
                    sector_number: pre_commit.info.sector_number,
                    seal_proof: pre_commit.info.seal_proof,
                    sealed_cid: pre_commit.info.sealed_cid,
                    deal_ids: pre_commit.info.deal_ids,
                    expiration: pre_commit.info.expiration,
                    activation,
                    deal_weight: pre_commit.deal_weight,
                    verified_deal_weight: pre_commit.verified_deal_weight,
                    initial_pledge,
                    expected_day_reward: day_reward,
                    expected_storage_pledge: storage_pledge,
                    replaced_sector_age,
                    replaced_day_reward,
                };

                new_sector_numbers.push(new_sector_info.sector_number);
                new_sectors.push(new_sector_info);
            }

            state.put_sectors(store, new_sectors.clone()).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to put new sectors")
            })?;

            state
                .delete_precommitted_sectors(store, &new_sector_numbers)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        "failed to delete precommited sectors",
                    )
                })?;

            state
                .assign_sectors_to_deadlines(
                    store,
                    rt.curr_epoch(),
                    new_sectors,
                    info.window_post_partition_sectors,
                    info.sector_size,
                )
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        "failed to assign new sectors to deadlines",
                    )
                })?;

            let newly_vested = TokenAmount::zero();

            // Unlock deposit for successful proofs, make it available for lock-up as initial pledge.
            state
                .add_pre_commit_deposit(&(-deposit_to_unlock))
                .map_err(|e| {
                    actor_error!(ErrIllegalState, "failed to add precommit deposit: {}", e)
                })?;

            let unlocked_balance =
                state
                    .get_unlocked_balance(&rt.current_balance()?)
                    .map_err(|e| {
                        actor_error!(
                            ErrIllegalState,
                            "failed to calculate unlocked balance: {}",
                            e
                        )
                    })?;
            if unlocked_balance < total_pledge {
                return Err(actor_error!(
                    ErrInsufficientFunds,
                    "insufficient funds for aggregate initial pledge requirement {}, available: {}",
                    total_pledge,
                    unlocked_balance
                ));
            }

            state.add_initial_pledge(&total_pledge).map_err(|e| {
                actor_error!(ErrIllegalState, "failed to add initial pledge: {}", e)
            })?;

            state
                .check_balance_invariants(&rt.current_balance()?)
                .map_err(|e| {
                    ActorError::new(
                        ErrBalanceInvariantBroken,
                        format!("balance invariant broken: {}", e),
                    )
                })?;

            Ok((total_pledge, newly_vested))
        })?;

        // Request pledge update for activated sector.
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

        if params.sector_number > MAX_SECTOR_NUMBER {
            return Err(actor_error!(
                ErrIllegalArgument,
                "sector number out of range"
            ));
        }

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
        mut params: ExtendSectorExpirationParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if params.extensions.len() as u64 > DELCARATIONS_MAX {
            return Err(actor_error!(
                ErrIllegalArgument,
                "too many declarations {}, max {}",
                params.extensions.len(),
                DELCARATIONS_MAX
            ));
        }

        // limit the number of sectors declared at once
        // https://github.com/filecoin-project/specs-actors/issues/416
        let mut sector_count: u64 = 0;

        for decl in &mut params.extensions {
            if decl.deadline >= WPOST_PERIOD_DEADLINES as usize {
                return Err(actor_error!(
                    ErrIllegalArgument,
                    "deadline {} not in range 0..{}",
                    decl.deadline,
                    WPOST_PERIOD_DEADLINES
                ));
            }

            let sectors = match decl.sectors.validate() {
                Ok(sectors) => sectors,
                Err(e) => {
                    return Err(actor_error!(
                        ErrIllegalArgument,
                        "failed to validate sectors for deadline {}, partition {}: {}",
                        decl.deadline,
                        decl.partition,
                        e
                    ))
                }
            };

            match sector_count.checked_add(sectors.len() as u64) {
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
            let info = get_miner_info(rt.store(), state)?;

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
            let mut decls_by_deadline = HashMap::<usize, Vec<ExpirationExtension>>::new();
            let mut deadlines_to_load = Vec::<usize>::new();

            for decl in params.extensions {
                decls_by_deadline
                    .entry(decl.deadline)
                    .or_insert_with(|| {
                        deadlines_to_load.push(decl.deadline);
                        Vec::new()
                    })
                    .push(decl);
            }

            let mut sectors = Sectors::load(rt.store(), &state.sectors).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to load sectors array")
            })?;

            let mut power_delta = PowerPair::zero();
            let mut pledge_delta = TokenAmount::zero();

            for deadline_idx in deadlines_to_load {
                let mut deadline = deadlines
                    .load_deadline(store, deadline_idx)
                    .map_err(|e| e.wrap(format!("failed to load deadline {}", deadline_idx)))?;

                let mut partitions = deadline.partitions_amt(store).map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to load partitions for deadline {}", deadline_idx),
                    )
                })?;

                let quant = state.quant_spec_for_deadline(deadline_idx);

                // Group modified partitions by epoch to which they are extended. Duplicates are ok.
                let mut partitions_by_new_epoch = HashMap::<ChainEpoch, Vec<usize>>::new();
                let mut epochs_to_reschedule = Vec::<ChainEpoch>::new();

                for decl in decls_by_deadline.get_mut(&deadline_idx).unwrap() {
                    let key = PartitionKey {
                        deadline: deadline_idx,
                        partition: decl.partition,
                    };

                    let mut partition = partitions
                        .get(decl.partition)
                        .map_err(|e| {
                            e.downcast_default(
                                ExitCode::ErrIllegalState,
                                format!("failed to load partition {:?}", key),
                            )
                        })?
                        .cloned()
                        .ok_or_else(|| actor_error!(ErrNotFound, "no such partition {:?}", key))?;

                    let old_sectors = sectors
                        .load_sector(&mut decl.sectors)
                        .map_err(|e| e.wrap("failed to load sectors"))?;

                    let new_sectors: Vec<SectorOnChainInfo> = old_sectors
                        .iter()
                        .map(|sector| {
                            if !can_extend_seal_proof_type(sector.seal_proof) {
                                return Err(actor_error!(
                                    ErrForbidden,
                                    "cannot extend expiration for sector {} with unsupported \
                                    seal type {:?}",
                                    sector.sector_number,
                                    sector.seal_proof
                                ));
                            }

                            // This can happen if the sector should have already expired, but hasn't
                            // because the end of its deadline hasn't passed yet.
                            if sector.expiration < rt.curr_epoch() {
                                return Err(actor_error!(
                                    ErrForbidden,
                                    "cannot extend expiration for expired sector {} at {}",
                                    sector.sector_number,
                                    sector.expiration
                                ));
                            }

                            if decl.new_expiration < sector.expiration {
                                return Err(actor_error!(
                                    ErrIllegalArgument,
                                    "cannot reduce sector {} expiration to {} from {}",
                                    sector.sector_number,
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
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            format!("failed to update sectors {:?}", decl.sectors),
                        )
                    })?;

                    // Remove old sectors from partition and assign new sectors.
                    let (partition_power_delta, partition_pledge_delta) = partition
                        .replace_sectors(store, &old_sectors, &new_sectors, info.sector_size, quant)
                        .map_err(|e| {
                            e.downcast_default(
                                ExitCode::ErrIllegalState,
                                format!("failed to replace sector expirations at {:?}", key),
                            )
                        })?;

                    power_delta += &partition_power_delta;
                    pledge_delta += partition_pledge_delta; // expected to be zero, see note below.

                    partitions.set(decl.partition, partition).map_err(|e| {
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            format!("failed to save partition {:?}", key),
                        )
                    })?;

                    // Record the new partition expiration epoch for setting outside this loop
                    // over declarations.
                    let prev_epoch_partitions = partitions_by_new_epoch.entry(decl.new_expiration);
                    let not_exists = matches!(prev_epoch_partitions, Entry::Vacant(_));

                    // Add declaration partition
                    prev_epoch_partitions
                        .or_insert_with(Vec::new)
                        .push(decl.partition);
                    if not_exists {
                        // reschedule epoch if the partition for new epoch didn't already exist
                        epochs_to_reschedule.push(decl.new_expiration);
                    }
                }

                deadline.partitions = partitions.flush().map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to save partitions for deadline {}", deadline_idx),
                    )
                })?;

                // Record partitions in deadline expiration queue
                for epoch in epochs_to_reschedule {
                    let p_idxs = partitions_by_new_epoch.get(&epoch).unwrap();
                    deadline
                        .add_expiration_partitions(store, epoch, p_idxs, quant)
                        .map_err(|e| {
                            e.downcast_default(
                                ExitCode::ErrIllegalState,
                                format!(
                                    "failed to add expiration partitions to \
                                        deadline {} epoch {}",
                                    deadline_idx, epoch
                                ),
                            )
                        })?;
                }

                deadlines
                    .update_deadline(store, deadline_idx, &deadline)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            format!("failed to save deadline {}", deadline_idx),
                        )
                    })?;
            }

            state.sectors = sectors.amt.flush().map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to save sectors")
            })?;
            state.save_deadlines(store, deadlines).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to save deadlines")
            })?;

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
    /// masked in the same way as faulty sectors. A miner may not terminate sectors in the
    /// current deadline or the next deadline to be proven.
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

        if params.terminations.len() as u64 > DELCARATIONS_MAX {
            return Err(actor_error!(
                ErrIllegalArgument,
                "too many declarations when terminating sectors: {} > {}",
                params.terminations.len(),
                DELCARATIONS_MAX
            ));
        }

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

            let info = get_miner_info(rt.store(), state)?;

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
            let sectors = Sectors::load(store, &state.sectors).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to load sectors")
            })?;

            for (deadline_idx, partition_sectors) in to_process.iter() {
                // If the deadline the current or next deadline to prove, don't allow terminating sectors.
                // We assume that deadlines are immutable when being proven.
                if !deadline_is_mutable(state.proving_period_start, deadline_idx, curr_epoch) {
                    return Err(actor_error!(
                        ErrIllegalArgument,
                        "cannot terminate sectors in immutable deadline {}",
                        deadline_idx
                    ));
                }

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
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            format!("failed to terminate sectors in deadline {}", deadline_idx),
                        )
                    })?;

                state.early_terminations.set(deadline_idx as usize);
                power_delta -= &removed_power;

                deadlines
                    .update_deadline(store, deadline_idx, &deadline)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            format!("failed to update deadline {}", deadline_idx),
                        )
                    })?;
            }

            state.save_deadlines(store, deadlines).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to save deadlines")
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
        let state: State = rt.state()?;
        state
            .check_balance_invariants(&rt.current_balance()?)
            .map_err(|e| {
                ActorError::new(
                    ErrBalanceInvariantBroken,
                    format!("balance invariant broken: {}", e),
                )
            })?;

        request_update_power(rt, power_delta)?;
        Ok(TerminateSectorsReturn { done: !more })
    }

    fn declare_faults<BS, RT>(rt: &mut RT, params: DeclareFaultsParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if params.faults.len() as u64 > DELCARATIONS_MAX {
            return Err(actor_error!(
                ErrIllegalArgument,
                "too many fault declarations for a single message: {} > {}",
                params.faults.len(),
                DELCARATIONS_MAX
            ));
        }

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

        let power_delta = rt.transaction(|state: &mut State, rt| {
            let info = get_miner_info(rt.store(), &state)?;

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
                e.downcast_default(ExitCode::ErrIllegalState, "failed to load sectors array")
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

                let deadline_power_delta = deadline
                    .record_faults(
                        store,
                        &sectors,
                        info.sector_size,
                        target_deadline.quant_spec(),
                        fault_expiration_epoch,
                        partition_map,
                    )
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            format!("failed to declare faults for deadline {}", deadline_idx),
                        )
                    })?;

                deadlines
                    .update_deadline(store, deadline_idx, &deadline)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            format!("failed to store deadline {} partitions", deadline_idx),
                        )
                    })?;

                new_fault_power_total += &deadline_power_delta;
            }

            state.save_deadlines(store, deadlines).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to save deadlines")
            })?;

            Ok(new_fault_power_total)
        })?;

        // Remove power for new faulty sectors.
        // NOTE: It would be permissible to delay the power loss until the deadline closes, but that would require
        // additional accounting state.
        // https://github.com/filecoin-project/specs-actors/issues/414
        request_update_power(rt, power_delta)?;

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
        if params.recoveries.len() as u64 > DELCARATIONS_MAX {
            return Err(actor_error!(
                ErrIllegalArgument,
                "too many recovery declarations for a single message: {} > {}",
                params.recoveries.len(),
                DELCARATIONS_MAX
            ));
        }

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

        let fee_to_burn = rt.transaction(|state: &mut State, rt| {
            // Verify unlocked funds cover both InitialPledgeRequirement and FeeDebt
            // and repay fee debt now.
            let fee_to_burn = repay_debts_or_abort(rt, state)?;

            let info = get_miner_info(rt.store(), &state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            if consensus_fault_active(&info, rt.curr_epoch()) {
                return Err(actor_error!(
                    ErrForbidden,
                    "recovery not allowed during active consensus fault"
                ));
            }

            let store = rt.store();

            let mut deadlines = state
                .load_deadlines(store)
                .map_err(|e| e.wrap("failed to load deadlines"))?;

            let sectors = Sectors::load(store, &state.sectors).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to load sectors array")
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
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            format!("failed to declare recoveries for deadline {}", deadline_idx),
                        )
                    })?;

                deadlines
                    .update_deadline(store, deadline_idx, &deadline)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::ErrIllegalState,
                            format!("failed to store deadline {}", deadline_idx),
                        )
                    })?;
            }

            state.save_deadlines(store, deadlines).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to save deadlines")
            })?;

            Ok(fee_to_burn)
        })?;

        burn_funds(rt, fee_to_burn)?;
        let state: State = rt.state()?;
        state
            .check_balance_invariants(&rt.current_balance()?)
            .map_err(|e| {
                ActorError::new(
                    ErrBalanceInvariantBroken,
                    format!("balance invariants broken: {}", e),
                )
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
        mut params: CompactPartitionsParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if params.deadline >= WPOST_PERIOD_DEADLINES as usize {
            return Err(actor_error!(
                ErrIllegalArgument,
                "invalid deadline {}",
                params.deadline
            ));
        }

        let partitions = params.partitions.validate().map_err(|e| {
            actor_error!(
                ErrIllegalArgument,
                "failed to parse partitions bitfield: {}",
                e
            )
        })?;
        let partition_count = partitions.len() as u64;

        let params_deadline = params.deadline;

        rt.transaction(|state: &mut State, rt| {
            let info = get_miner_info(rt.store(), state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            let store = rt.store();

            if !deadline_available_for_compaction(
                state.proving_period_start,
                params_deadline,
                rt.curr_epoch(),
            ) {
                return Err(actor_error!(
                    ErrForbidden,
                    "cannot compact deadline {} during its challenge window, \
                    or the prior challenge window, 
                    or before {} epochs have passed since its last challenge window ended",
                    params_deadline,
                    WPOST_DISPUTE_WINDOW
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

            let quant = state.quant_spec_for_deadline(params_deadline);
            let mut deadlines = state
                .load_deadlines(store)
                .map_err(|e| e.wrap("failed to load deadlines"))?;

            let mut deadline = deadlines
                .load_deadline(store, params_deadline)
                .map_err(|e| e.wrap(format!("failed to load deadline {}", params_deadline)))?;

            let (live, dead, removed_power) = deadline
                .remove_partitions(store, partitions, quant)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!(
                            "failed to remove partitions from deadline {}",
                            params_deadline
                        ),
                    )
                })?;

            state.delete_sectors(store, &dead).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to delete dead sectors")
            })?;

            let sectors = state.load_sector_infos(store, &live).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to load moved sectors")
            })?;
            let proven = true;
            let added_power = deadline
                .add_sectors(
                    store,
                    info.window_post_partition_sectors,
                    proven,
                    &sectors,
                    info.sector_size,
                    quant,
                )
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        "failed to add back moved sectors",
                    )
                })?;

            if removed_power != added_power {
                return Err(actor_error!(
                    ErrIllegalState,
                    "power changed when compacting partitions: was {:?}, is now {:?}",
                    removed_power,
                    added_power
                ));
            }

            deadlines
                .update_deadline(store, params_deadline, &deadline)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to update deadline {}", params_deadline),
                    )
                })?;

            state.save_deadlines(store, deadlines).map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    format!("failed to save deadline {}", params_deadline),
                )
            })?;

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
        mut params: CompactSectorNumbersParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let mask_sector_numbers = params
            .mask_sector_numbers
            .validate()
            .map_err(|e| actor_error!(ErrIllegalArgument, "invalid mask bitfield: {}", e))?;

        let last_sector_number = mask_sector_numbers
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
            let info = get_miner_info(rt.store(), state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            state.mask_sector_numbers(rt.store(), mask_sector_numbers)
        })?;

        Ok(())
    }

    /// Locks up some amount of a the miner's unlocked balance (including funds received alongside the invoking message).
    fn apply_rewards<BS, RT>(rt: &mut RT, params: ApplyRewardParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if params.reward.is_negative() {
            return Err(actor_error!(
                ErrIllegalArgument,
                "cannot lock up a negative amount of funds"
            ));
        }
        if params.penalty.is_negative() {
            return Err(actor_error!(
                ErrIllegalArgument,
                "cannot penalize a negative amount of funds"
            ));
        }

        let (pledge_delta_total, to_burn) = rt.transaction(|st: &mut State, rt| {
            let mut pledge_delta_total = TokenAmount::zero();

            rt.validate_immediate_caller_is(std::iter::once(&*REWARD_ACTOR_ADDR))?;

            let (reward_to_lock, locked_reward_vesting_spec) =
                locked_reward_from_reward(params.reward);

            // This ensures the miner has sufficient funds to lock up amountToLock.
            // This should always be true if reward actor sends reward funds with the message.
            let unlocked_balance =
                st.get_unlocked_balance(&rt.current_balance()?)
                    .map_err(|e| {
                        actor_error!(
                            ErrIllegalState,
                            "failed to calculate unlocked balance: {}",
                            e
                        )
                    })?;

            if unlocked_balance < reward_to_lock {
                return Err(actor_error!(
                    ErrInsufficientFunds,
                    "insufficient funds to lock, available: {}, requested: {}",
                    unlocked_balance,
                    reward_to_lock
                ));
            }

            let newly_vested = st
                .add_locked_funds(
                    rt.store(),
                    rt.curr_epoch(),
                    &reward_to_lock,
                    locked_reward_vesting_spec,
                )
                .map_err(|e| {
                    actor_error!(
                        ErrIllegalState,
                        "failed to lock funds in vesting table: {}",
                        e
                    )
                })?;
            pledge_delta_total -= &newly_vested;
            pledge_delta_total += &reward_to_lock;

            st.apply_penalty(&params.penalty)
                .map_err(|e| actor_error!(ErrIllegalState, "failed to apply penalty: {}", e))?;

            // Attempt to repay all fee debt in this call. In most cases the miner will have enough
            // funds in the *reward alone* to cover the penalty. In the rare case a miner incurs more
            // penalty than it can pay for with reward and existing funds, it will go into fee debt.
            let (penalty_from_vesting, penalty_from_balance) = st
                .repay_partial_debt_in_priority_order(
                    rt.store(),
                    rt.curr_epoch(),
                    &rt.current_balance()?,
                )
                .map_err(|e| actor_error!(ErrIllegalState, "failed to repay penalty: {}", e))?;
            pledge_delta_total -= &penalty_from_vesting;
            let to_burn = penalty_from_vesting + penalty_from_balance;
            Ok((pledge_delta_total, to_burn))
        })?;

        notify_pledge_changed(rt, &pledge_delta_total)?;
        burn_funds(rt, to_burn)?;
        let st: State = rt.state()?;
        st.check_balance_invariants(&rt.current_balance()?)
            .map_err(|e| {
                ActorError::new(
                    ErrBalanceInvariantBroken,
                    format!("balance invariants broken: {}", e),
                )
            })?;
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
        // Note: only the first report of any fault is processed because it sets the
        // ConsensusFaultElapsed state variable to an epoch after the fault, and reports prior to
        // that epoch are no longer valid
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;
        let reporter = *rt.message().caller();

        let fault = rt
            .verify_consensus_fault(&params.header1, &params.header2, &params.header_extra)
            .map_err(|e| e.downcast_default(ExitCode::ErrIllegalArgument, "fault not verified"))?
            .ok_or_else(|| actor_error!(ErrIllegalArgument, "No consensus fault found"))?;
        if fault.target != *rt.message().receiver() {
            return Err(actor_error!(
                ErrIllegalArgument,
                "fault by {} reported to miner {}",
                fault.target,
                rt.message().receiver()
            ));
        }

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
        let reward_stats = request_current_epoch_block_reward(rt)?;

        // The policy amounts we should burn and send to reporter
        // These may differ from actual funds send when miner goes into fee debt
        let fault_penalty =
            consensus_fault_penalty(reward_stats.this_epoch_reward_smoothed.estimate());
        let slasher_reward = reward_for_consensus_slash_report(fault_age, &fault_penalty);

        let mut pledge_delta = TokenAmount::from(0);

        let (burn_amount, reward_amount) = rt.transaction(|st: &mut State, rt| {
            let mut info = get_miner_info(rt.store(), &st)?;

            // Verify miner hasn't already been faulted
            if fault.epoch < info.consensus_fault_elapsed {
                return Err(actor_error!(
                    ErrForbidden,
                    "fault epoch {} is too old, last exclusion period ended at {}",
                    fault.epoch,
                    info.consensus_fault_elapsed
                ));
            }

            st.apply_penalty(&fault_penalty).map_err(|e| {
                actor_error!(ErrIllegalState, format!("failed to apply penalty: {}", e))
            })?;

            // Pay penalty
            let (penalty_from_vesting, penalty_from_balance) = st
                .repay_partial_debt_in_priority_order(
                    rt.store(),
                    rt.curr_epoch(),
                    &rt.current_balance()?,
                )
                .map_err(|e| e.downcast_default(ExitCode::ErrIllegalState, "failed to pay fees"))?;

            let mut burn_amount = &penalty_from_vesting + &penalty_from_balance;
            pledge_delta -= penalty_from_vesting;

            // clamp reward at funds burnt
            let reward_amount = std::cmp::min(&burn_amount, &slasher_reward).clone();
            burn_amount -= &reward_amount;

            info.consensus_fault_elapsed = rt.curr_epoch() + CONSENSUS_FAULT_INELIGIBILITY_DURATION;

            st.save_info(rt.store(), &info).map_err(|e| {
                e.downcast_default(ExitCode::ErrSerialization, "failed to save miner info")
            })?;

            Ok((burn_amount, reward_amount))
        })?;

        if let Err(e) = rt.send(reporter, METHOD_SEND, Serialized::default(), reward_amount) {
            log::error!("failed to send reward: {}", e);
        }

        burn_funds(rt, burn_amount)?;
        notify_pledge_changed(rt, &pledge_delta)?;

        let state: State = rt.state()?;
        state
            .check_balance_invariants(&rt.current_balance()?)
            .map_err(|e| {
                ActorError::new(
                    ErrBalanceInvariantBroken,
                    format!("balance invariants broken: {}", e),
                )
            })?;
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

        let (info, newly_vested, fee_to_burn, available_balance, state) =
            rt.transaction(|state: &mut State, rt| {
                let info = get_miner_info(rt.store(), state)?;

                // Only the owner is allowed to withdraw the balance as it belongs to/is controlled by the owner
                // and not the worker.
                rt.validate_immediate_caller_is(&[info.owner])?;

                // Ensure we don't have any pending terminations.
                if !state.early_terminations.is_empty() {
                    return Err(actor_error!(
                        ErrForbidden,
                        "cannot withdraw funds while {} deadlines have terminated sectors \
                        with outstanding fees",
                        state.early_terminations.len()
                    ));
                }

                // Unlock vested funds so we can spend them.
                let newly_vested = state
                    .unlock_vested_funds(rt.store(), rt.curr_epoch())
                    .map_err(|e| {
                        e.downcast_default(ExitCode::ErrIllegalState, "Failed to vest fund")
                    })?;

                // available balance already accounts for fee debt so it is correct to call
                // this before RepayDebts. We would have to
                // subtract fee debt explicitly if we called this after.
                let available_balance = state
                    .get_available_balance(&rt.current_balance()?)
                    .map_err(|e| {
                        actor_error!(
                            ErrIllegalState,
                            format!("failed to calculate available balance: {}", e)
                        )
                    })?;

                // Verify unlocked funds cover both InitialPledgeRequirement and FeeDebt
                // and repay fee debt now.
                let fee_to_burn = repay_debts_or_abort(rt, state)?;

                Ok((
                    info,
                    newly_vested,
                    fee_to_burn,
                    available_balance,
                    state.clone(),
                ))
            })?;

        let amount_withdrawn = std::cmp::min(&available_balance, &params.amount_requested);
        assert!(!amount_withdrawn.is_negative());
        if amount_withdrawn.is_negative() {
            return Err(actor_error!(
                ErrIllegalState,
                "negative amount to withdraw: {}",
                amount_withdrawn
            ));
        }
        if amount_withdrawn > &available_balance {
            return Err(actor_error!(
                ErrIllegalState,
                "amount to withdraw {} < available {}",
                amount_withdrawn,
                available_balance
            ));
        }

        if amount_withdrawn.is_positive() {
            rt.send(
                info.owner,
                METHOD_SEND,
                Serialized::default(),
                amount_withdrawn.clone(),
            )?;
        }

        burn_funds(rt, fee_to_burn)?;
        notify_pledge_changed(rt, &newly_vested.neg())?;

        state
            .check_balance_invariants(&rt.current_balance()?)
            .map_err(|e| {
                ActorError::new(
                    ErrBalanceInvariantBroken,
                    format!("balance invariants broken: {}", e),
                )
            })?;
        Ok(())
    }

    fn repay_debt<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let (from_vesting, from_balance, state) = rt.transaction(|state: &mut State, rt| {
            let info = get_miner_info(rt.store(), state)?;
            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            // Repay as much fee debt as possible.
            let (from_vesting, from_balance) = state
                .repay_partial_debt_in_priority_order(
                    rt.store(),
                    rt.curr_epoch(),
                    &rt.current_balance()?,
                )
                .map_err(|e| {
                    e.downcast_default(ExitCode::ErrIllegalState, "failed to unlock fee debt")
                })?;

            Ok((from_vesting, from_balance, state.clone()))
        })?;

        let burn_amount = from_balance + &from_vesting;
        notify_pledge_changed(rt, &from_vesting.neg())?;
        burn_funds(rt, burn_amount)?;

        state
            .check_balance_invariants(&rt.current_balance()?)
            .map_err(|e| {
                ActorError::new(
                    ErrBalanceInvariantBroken,
                    format!("balance invariants broken: {}", e),
                )
            })?;
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
        rt.validate_immediate_caller_is(std::iter::once(&*STORAGE_POWER_ACTOR_ADDR))?;

        match payload.event_type {
            CRON_EVENT_PROVING_DEADLINE => handle_proving_deadline(rt)?,
            CRON_EVENT_PROCESS_EARLY_TERMINATIONS => {
                if process_early_terminations(rt)? {
                    schedule_early_termination_work(rt)?
                }
            }
            _ => {}
        };
        let state: State = rt.state()?;
        state
            .check_balance_invariants(&rt.current_balance()?)
            .map_err(|e| {
                ActorError::new(
                    ErrBalanceInvariantBroken,
                    format!("balance invariants broken: {}", e),
                )
            })?;
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
                    e.downcast_default(
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

            let info = get_miner_info(rt.store(), state)?;
            let sectors = Sectors::load(store, &state.sectors).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to load sectors array")
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

            // Pay penalty
            state
                .apply_penalty(&penalty)
                .map_err(|e| actor_error!(ErrIllegalState, "failed to apply penalty: {}", e))?;

            // Remove pledge requirement.
            let mut pledge_delta = -total_initial_pledge;
            state.add_initial_pledge(&pledge_delta).map_err(|e| {
                actor_error!(
                    ErrIllegalState,
                    "failed to add initial pledge {}: {}",
                    pledge_delta,
                    e
                )
            })?;

            // Use unlocked pledge to pay down outstanding fee debt
            let (penalty_from_vesting, penalty_from_balance) = state
                .repay_partial_debt_in_priority_order(
                    rt.store(),
                    rt.curr_epoch(),
                    &rt.current_balance()?,
                )
                .map_err(|e| actor_error!(ErrIllegalState, "failed to repay penalty: {}", e))?;

            penalty = &penalty_from_vesting + penalty_from_balance;
            pledge_delta -= penalty_from_vesting;

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

    let mut power_delta_total = PowerPair::zero();
    let mut penalty_total = TokenAmount::zero();
    let mut pledge_delta_total = TokenAmount::zero();

    let state: State = rt.transaction(|state: &mut State, rt| {
        // Vest locked funds.
        // This happens first so that any subsequent penalties are taken
        // from locked vesting funds before funds free this epoch.
        let newly_vested = state
            .unlock_vested_funds(rt.store(), rt.curr_epoch())
            .map_err(|e| e.downcast_default(ExitCode::ErrIllegalState, "failed to vest funds"))?;

        pledge_delta_total -= newly_vested;

        // Process pending worker change if any
        let mut info = get_miner_info(rt.store(), &state)?;
        process_pending_worker(&mut info, rt, state)?;

        let deposit_to_burn = state
            .expire_pre_commits(rt.store(), rt.curr_epoch())
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to expire pre-committed sectors",
                )
            })?;

        state
            .apply_penalty(&deposit_to_burn)
            .map_err(|e| actor_error!(ErrIllegalState, "failed to apply penalty: {}", e))?;

        // Record whether or not we _had_ early terminations in the queue before this method.
        // That way, don't re-schedule a cron callback if one is already scheduled.
        had_early_terminations = have_pending_early_terminations(state);

        let result = state
            .advance_deadline(rt.store(), rt.curr_epoch())
            .map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to advance deadline")
            })?;

        // Faults detected by this missed PoSt pay no penalty, but sectors that were already faulty
        // and remain faulty through this deadline pay the fault fee.
        let penalty_target = pledge_penalty_for_continued_fault(
            &epoch_reward.this_epoch_reward_smoothed,
            &power_total.quality_adj_power_smoothed,
            &result.previously_faulty_power.qa,
        );

        power_delta_total += &result.power_delta;
        pledge_delta_total += &result.pledge_delta;

        state
            .apply_penalty(&penalty_target)
            .map_err(|e| actor_error!(ErrIllegalState, "failed to apply penalty: {}", e))?;

        let (penalty_from_vesting, penalty_from_balance) = state
            .repay_partial_debt_in_priority_order(
                rt.store(),
                rt.curr_epoch(),
                &rt.current_balance()?,
            )
            .map_err(|e| actor_error!(ErrIllegalState, "failed to unlock penalty: {}", e))?;

        penalty_total = &penalty_from_vesting + penalty_from_balance;
        pledge_delta_total -= penalty_from_vesting;
        Ok(state.clone())
    })?;

    // Remove power for new faults, and burn penalties.
    request_update_power(rt, power_delta_total)?;
    burn_funds(rt, penalty_total)?;
    notify_pledge_changed(rt, &pledge_delta_total)?;

    // Schedule cron callback for next deadline's last epoch.
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
    // Expiration must be after activation. Check this explicitly to avoid an underflow below.
    if expiration <= activation {
        return Err(actor_error!(
            ErrIllegalArgument,
            "sector expiration {} must be after activation {}",
            expiration,
            activation
        ));
    }

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
    let max_lifetime = seal_proof_sector_maximum_lifetime(seal_proof).ok_or_else(|| {
        actor_error!(
            ErrIllegalArgument,
            "unrecognized seal proof type {:?}",
            seal_proof
        )
    })?;
    if expiration - activation > max_lifetime {
        return Err(actor_error!(
            ErrIllegalArgument,
            "invalid expiration {}, total sector lifetime ({}) cannot exceed {} after activation {}",
            expiration,
            expiration - activation,
            max_lifetime,
            activation
        ));
    }

    Ok(())
}

fn validate_replace_sector<BS>(
    state: &State,
    store: &BS,
    params: &SectorPreCommitInfo,
) -> Result<(), ActorError>
where
    BS: BlockStore,
{
    let replace_sector = state
        .get_sector(store, params.replace_sector_number)
        .map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                format!("failed to load sector {}", params.sector_number),
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

    // From network version 7, the new sector's seal type must have the same Window PoSt proof type as the one
    // being replaced, rather than be exactly the same seal type.
    // This permits replacing sectors with V1 seal types with V1_1 seal types.
    let replace_w_post_proof = replace_sector
        .seal_proof
        .registered_window_post_proof()
        .map_err(|e| {
            actor_error!(
                ErrIllegalState,
                "failed to lookup Window PoSt proof type for sector seal proof {:?}: {}",
                replace_sector.seal_proof,
                e
            )
        })?;
    let new_w_post_proof = params
        .seal_proof
        .registered_window_post_proof()
        .map_err(|e| {
            actor_error!(
                ErrIllegalArgument,
                "failed to lookup Window PoSt proof type for new seal proof {:?}: {}",
                replace_sector.seal_proof,
                e
            )
        })?;

    if replace_w_post_proof != new_w_post_proof {
        return Err(actor_error!(
                ErrIllegalArgument,
                "new sector window PoSt proof type {:?} must match replaced proof type {:?} (seal proof type {:?})",
                replace_w_post_proof,
                new_w_post_proof,
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
            e.downcast_default(
                ExitCode::ErrIllegalState,
                format!("failed to replace sector {}", params.replace_sector_number),
            )
        })?;

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
    let payload = Serialized::serialize(cb)
        .map_err(|e| ActorError::from(e).wrap("failed to serialize payload: {}"))?;

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
        return Err(actor_error!(
            ErrIllegalState,
            "runtime provided bad receiver address {}",
            rt.message().receiver()
        ));
    };

    // Regenerate challenge randomness, which must match that generated for the proof.
    let entropy = rt.message().receiver().marshal_cbor().map_err(|e| {
        ActorError::from(e).wrap("failed to marshal address for window post challenge")
    })?;
    let randomness: PoStRandomness =
        rt.get_randomness_from_beacon(WindowedPoStChallengeSeed, challenge_epoch, &entropy)?;

    let challenged_sectors = sectors
        .iter()
        .map(|s| SectorInfo {
            proof: s.seal_proof,
            sector_number: s.sector_number,
            sealed_cid: s.sealed_cid,
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
    rt.verify_post(&pv_info).map_err(|e| {
        e.downcast_default(
            ExitCode::ErrIllegalArgument,
            format!(
                "invalid PoSt: proofs({:?}), randomness({:?})",
                pv_info.proofs, pv_info.randomness
            ),
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
        return Err(actor_error!(ErrForbidden, "too early to prove sector"));
    }

    let commd = request_unsealed_sector_cid(rt, params.registered_seal_proof, &params.deal_ids)?;

    let miner_actor_id: u64 = if let Payload::ID(i) = rt.message().receiver().payload() {
        *i
    } else {
        return Err(actor_error!(
            ErrIllegalState,
            "runtime provided non ID receiver address {}",
            rt.message().receiver()
        ));
    };
    let entropy =
        rt.message().receiver().marshal_cbor().map_err(|e| {
            ActorError::from(e).wrap("failed to marshal address for get verify info")
        })?;
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

fn request_deal_weights<BS, RT>(
    rt: &mut RT,
    sectors: &[market::SectorDeals],
) -> Result<VerifyDealsForActivationReturn, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    // Short-circuit if there are no deals in any of the sectors.
    let mut deal_count = 0;
    for sector in sectors {
        deal_count += sector.deal_ids.len();
    }
    if deal_count == 0 {
        let mut empty_result = VerifyDealsForActivationReturn {
            sectors: Vec::with_capacity(sectors.len()),
        };
        for _ in 0..sectors.len() {
            empty_result.sectors.push(market::SectorWeights {
                deal_space: 0,
                deal_weight: 0.into(),
                verified_deal_weight: 0.into(),
            });
        }
        return Ok(empty_result);
    }
    let serialized = rt.send(
        *STORAGE_MARKET_ACTOR_ADDR,
        MarketMethod::VerifyDealsForActivation as u64,
        Serialized::serialize(VerifyDealsForActivationParamsRef { sectors })?,
        TokenAmount::zero(),
    )?;

    Ok(serialized.deserialize()?)
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

    let ret: ThisEpochRewardReturn = ret
        .deserialize()
        .map_err(|e| ActorError::from(e).wrap("failed to unmarshal target power value"))?;

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

    let power: CurrentTotalPowerReturn = ret
        .deserialize()
        .map_err(|e| ActorError::from(e).wrap("failed to unmarshal power total value"))?;

    Ok(power)
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

    let worker_code = rt
        .get_actor_code_cid(&resolved)?
        .ok_or_else(|| actor_error!(ErrIllegalArgument, "no code for address: {}", resolved))?;
    if worker_code != *ACCOUNT_ACTOR_CODE_ID {
        return Err(actor_error!(
            ErrIllegalArgument,
            "worker actor type must be an account, was {}",
            worker_code
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
            ActorError::from(e).wrap(format!("failed to deserialize address result: {:?}", ret))
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
    my_addr.write_i64::<BigEndian>(current_epoch)?;

    let digest = blake2b(&my_addr)?;

    let mut offset: u64 = BigEndian::read_u64(&digest);
    offset %= WPOST_PROVING_PERIOD as u64;

    // Conversion from i64 to u64 is safe because it's % WPOST_PROVING_PERIOD which is i64
    Ok(offset as ChainEpoch)
}

/// Computes the epoch at which a proving period should start such that it is greater than the current epoch, and
/// has a defined offset from being an exact multiple of WPoStProvingPeriod.
/// A miner is exempt from Winow PoSt until the first full proving period starts.
fn current_proving_period_start(current_epoch: ChainEpoch, offset: ChainEpoch) -> ChainEpoch {
    let curr_modulus = current_epoch % WPOST_PROVING_PERIOD;

    let period_progress = if curr_modulus >= offset {
        curr_modulus - offset
    } else {
        WPOST_PROVING_PERIOD - (offset - curr_modulus)
    };

    current_epoch - period_progress
}

fn current_deadline_index(current_epoch: ChainEpoch, period_start: ChainEpoch) -> usize {
    ((current_epoch - period_start) / WPOST_CHALLENGE_WINDOW) as usize
}

/// Computes deadline information for a fault or recovery declaration.
/// If the deadline has not yet elapsed, the declaration is taken as being for the current proving period.
/// If the deadline has elapsed, it's instead taken as being for the next proving period after the current epoch.
fn declaration_deadline_info(
    period_start: ChainEpoch,
    deadline_idx: usize,
    current_epoch: ChainEpoch,
) -> Result<DeadlineInfo, String> {
    if deadline_idx >= WPOST_PERIOD_DEADLINES as usize {
        return Err(format!(
            "invalid deadline {}, must be < {}",
            deadline_idx, WPOST_PERIOD_DEADLINES
        ));
    }

    let deadline = new_deadline_info(period_start, deadline_idx, current_epoch).next_not_elapsed();
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
    sectors: &mut UnvalidatedBitField,
) -> Result<(), String> {
    let sectors = sectors
        .validate()
        .map_err(|e| format!("failed to check sectors: {}", e))?;

    // Check that the declared sectors are actually assigned to the partition.
    if partition.sectors.contains_all(sectors) {
        Ok(())
    } else {
        Err("not all sectors are assigned to the partition".to_string())
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
            current_epoch - sector.activation,
            &sector.expected_storage_pledge,
            network_qa_power_estimate,
            &sector_power,
            reward_estimate,
            &sector.replaced_day_reward,
            sector.replaced_sector_age,
        );
        total_fee += fee;
    }

    total_fee
}

fn consensus_fault_active(info: &MinerInfo, curr_epoch: ChainEpoch) -> bool {
    // For penalization period to last for exactly finality epochs
    // consensus faults are active until currEpoch exceeds ConsensusFaultElapsed
    curr_epoch <= info.consensus_fault_elapsed
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

fn get_miner_info<BS>(store: &BS, state: &State) -> Result<MinerInfo, ActorError>
where
    BS: BlockStore,
{
    state
        .get_info(store)
        .map_err(|e| e.downcast_default(ExitCode::ErrIllegalState, "could not read miner info"))
}

fn process_pending_worker<BS, RT>(
    info: &mut MinerInfo,
    rt: &RT,
    state: &mut State,
) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let pending_worker_key = if let Some(k) = &info.pending_worker_key {
        k
    } else {
        return Ok(());
    };

    if rt.curr_epoch() < pending_worker_key.effective_at {
        return Ok(());
    }

    info.worker = pending_worker_key.new_worker;
    info.pending_worker_key = None;

    state
        .save_info(rt.store(), &info)
        .map_err(|e| e.downcast_default(ExitCode::ErrIllegalState, "failed to save miner info"))
}

/// Repays all fee debt and then verifies that the miner has amount needed to cover
/// the pledge requirement after burning all fee debt.  If not aborts.
/// Returns an amount that must be burnt by the actor.
/// Note that this call does not compute recent vesting so reported unlocked balance
/// may be slightly lower than the true amount. Computing vesting here would be
/// almost always redundant since vesting is quantized to ~daily units.  Vesting
/// will be at most one proving period old if computed in the cron callback.
fn repay_debts_or_abort<BS, RT>(rt: &RT, state: &mut State) -> Result<TokenAmount, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    state.repay_debts(&rt.current_balance()?).map_err(|e| {
        e.downcast_default(
            ExitCode::ErrIllegalState,
            "unlocked balance ca not repay fee debt",
        )
    })
}

fn replaced_sector_parameters(
    curr_epoch: ChainEpoch,
    precommit: &SectorPreCommitOnChainInfo,
    replaced_by_num: &HashMap<SectorNumber, SectorOnChainInfo>,
) -> Result<(TokenAmount, ChainEpoch, TokenAmount), ActorError> {
    if !precommit.info.replace_capacity {
        return Ok(Default::default());
    }

    let replaced = replaced_by_num
        .get(&precommit.info.replace_sector_number)
        .ok_or_else(|| {
            actor_error!(
                ErrNotFound,
                "no such sector {} to replace",
                precommit.info.replace_sector_number
            )
        })?;

    let age = std::cmp::max(0, curr_epoch - replaced.activation);

    // The sector will actually be active for the period between activation and its next
    // proving deadline, but this covers the period for which we will be looking to the old sector
    // for termination fees.
    Ok((
        replaced.initial_pledge.clone(),
        age,
        replaced.expected_day_reward.clone(),
    ))
}

fn check_control_addresses(control_addrs: &[Address]) -> Result<(), ActorError> {
    if control_addrs.len() > MAX_CONTROL_ADDRESSES {
        return Err(actor_error!(
            ErrIllegalArgument,
            "control addresses length {} exceeds max control addresses length {}",
            control_addrs.len(),
            MAX_CONTROL_ADDRESSES
        ));
    }

    Ok(())
}

fn check_peer_info(peer_id: &[u8], multiaddrs: &[BytesDe]) -> Result<(), ActorError> {
    if peer_id.len() > MAX_PEER_ID_LENGTH {
        return Err(actor_error!(
            ErrIllegalArgument,
            "peer ID size of {} exceeds maximum size of {}",
            peer_id.len(),
            MAX_PEER_ID_LENGTH
        ));
    }

    let mut total_size = 0;
    for ma in multiaddrs {
        if ma.0.is_empty() {
            return Err(actor_error!(ErrIllegalArgument, "invalid empty multiaddr"));
        }
        total_size += ma.0.len();
    }

    if total_size > MAX_MULTIADDR_DATA {
        return Err(actor_error!(
            ErrIllegalArgument,
            "multiaddr size of {} exceeds maximum of {}",
            total_size,
            MAX_MULTIADDR_DATA
        ));
    }

    Ok(())
}

impl ActorCode for Actor {
    fn invoke_method<BS, RT>(
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
                Self::constructor(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::ControlAddresses) => {
                check_empty_params(params)?;
                let res = Self::control_addresses(rt)?;
                Ok(Serialized::serialize(&res)?)
            }
            Some(Method::ChangeWorkerAddress) => {
                Self::change_worker_address(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::ChangePeerID) => {
                Self::change_peer_id(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::SubmitWindowedPoSt) => {
                Self::submit_windowed_post(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::PreCommitSector) => {
                Self::pre_commit_sector(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::ProveCommitSector) => {
                Self::prove_commit_sector(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::ExtendSectorExpiration) => {
                Self::extend_sector_expiration(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::TerminateSectors) => {
                let ret = Self::terminate_sectors(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::serialize(ret)?)
            }
            Some(Method::DeclareFaults) => {
                Self::declare_faults(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::DeclareFaultsRecovered) => {
                Self::declare_faults_recovered(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::OnDeferredCronEvent) => {
                Self::on_deferred_cron_event(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::CheckSectorProven) => {
                Self::check_sector_proven(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::ApplyRewards) => {
                Self::apply_rewards(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::ReportConsensusFault) => {
                Self::report_consensus_fault(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::WithdrawBalance) => {
                Self::withdraw_balance(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::ConfirmSectorProofsValid) => {
                Self::confirm_sector_proofs_valid(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::ChangeMultiaddrs) => {
                Self::change_multiaddresses(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::CompactPartitions) => {
                Self::compact_partitions(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::CompactSectorNumbers) => {
                Self::compact_sector_numbers(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::ConfirmUpdateWorkerKey) => {
                check_empty_params(params)?;
                Self::confirm_update_worker_key(rt)?;
                Ok(Serialized::default())
            }
            Some(Method::RepayDebt) => {
                check_empty_params(params)?;
                Self::repay_debt(rt)?;
                Ok(Serialized::default())
            }
            Some(Method::ChangeOwnerAddress) => {
                Self::change_owner_address(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::DisputeWindowedPoSt) => {
                Self::dispute_windowed_post(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            None => Err(actor_error!(SysErrInvalidMethod, "Invalid method")),
        }
    }
}
