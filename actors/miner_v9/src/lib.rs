// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::iter;
use std::ops::Neg;

use anyhow::{anyhow, Error};
use byteorder::{BigEndian, ByteOrder, WriteBytesExt};
use cid::Cid;
use fvm_ipld_bitfield::{BitField, Validate};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{from_slice, BytesDe, Cbor, CborStore, RawBytes};
use fvm_shared::address::{Address, Payload, Protocol};
use fvm_shared::bigint::{BigInt, Integer};
use fvm_shared::clock::ChainEpoch;
use fvm_shared::deal::DealID;
use fvm_shared::econ::TokenAmount;
use fvm_shared::error::*;
use fvm_shared::randomness::*;
use fvm_shared::reward::ThisEpochRewardReturn;
use fvm_shared::sector::*;
use fvm_shared::smooth::FilterEstimate;
use fvm_shared::{MethodNum, METHOD_CONSTRUCTOR, METHOD_SEND};
use log::{error, info, warn};
use multihash::Code::Blake2b256;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Zero};

pub use beneficiary::*;
pub use bitfield_queue::*;
pub use commd::*;
pub use deadline_assignment::*;
pub use deadline_info::*;
pub use deadline_state::*;
pub use deadlines::*;
pub use expiration_queue::*;
use fil_actors_runtime_v9::cbor::{deserialize, serialize, serialize_vec};
use fil_actors_runtime_v9::runtime::builtins::Type;
use fil_actors_runtime_v9::runtime::{ActorCode, DomainSeparationTag, Policy, Runtime};
use fil_actors_runtime_v9::{
    actor_error, cbor, ActorContext, ActorDowncast, ActorError, BURNT_FUNDS_ACTOR_ADDR,
    CALLER_TYPES_SIGNABLE, INIT_ACTOR_ADDR, REWARD_ACTOR_ADDR, STORAGE_MARKET_ACTOR_ADDR,
    STORAGE_POWER_ACTOR_ADDR, VERIFIED_REGISTRY_ACTOR_ADDR,
};
pub use monies::*;
pub use partition_state::*;
pub use policy::*;
pub use sector_map::*;
pub use sectors::*;
pub use state::*;
pub use termination::*;
pub use types::*;
pub use vesting_state::*;

// The following errors are particular cases of illegal state.
// They're not expected to ever happen, but if they do, distinguished codes can help us
// diagnose the problem.

#[cfg(feature = "fil-actor")]
fil_actors_runtime_v9::wasm_trampoline!(Actor);

mod beneficiary;
mod bitfield_queue;
mod commd;
mod deadline_assignment;
mod deadline_info;
mod deadline_state;
mod deadlines;
mod expiration_queue;
#[doc(hidden)]
pub mod ext;
mod monies;
mod partition_state;
mod policy;
mod sector_map;
mod sectors;
mod state;
mod termination;
pub mod testing;
mod types;
mod vesting_state;

// The first 1000 actor-specific codes are left open for user error, i.e. things that might
// actually happen without programming error in the actor code.

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
    PreCommitSectorBatch = 25,
    ProveCommitAggregate = 26,
    ProveReplicaUpdates = 27,
    PreCommitSectorBatch2 = 28,
    ProveReplicaUpdates2 = 29,
    ChangeBeneficiary = 30,
    GetBeneficiary = 31,
    ExtendSectorExpiration2 = 32,
}

pub const ERR_BALANCE_INVARIANTS_BROKEN: ExitCode = ExitCode::new(1000);

/// Miner Actor
/// here in order to update the Power Actor to v3.
pub struct Actor;

impl Actor {
    pub fn constructor<BS, RT>(
        rt: &mut RT,
        params: MinerConstructorParams,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&INIT_ACTOR_ADDR))?;

        check_control_addresses(rt.policy(), &params.control_addresses)?;
        check_peer_info(rt.policy(), &params.peer_id, &params.multi_addresses)?;
        check_valid_post_proof_type(rt.policy(), params.window_post_proof_type)?;

        let owner = resolve_control_address(rt, params.owner)?;
        let worker = resolve_worker_address(rt, params.worker)?;
        let control_addresses: Vec<_> = params
            .control_addresses
            .into_iter()
            .map(|address| resolve_control_address(rt, address))
            .collect::<Result<_, _>>()?;

        let policy = rt.policy();
        let current_epoch = rt.curr_epoch();
        let blake2b = |b: &[u8]| rt.hash_blake2b(b);
        let offset =
            assign_proving_period_offset(policy, rt.message().receiver(), current_epoch, blake2b)
                .map_err(|e| {
                e.downcast_default(
                    ExitCode::USR_SERIALIZATION,
                    "failed to assign proving period offset",
                )
            })?;

        let period_start = current_proving_period_start(policy, current_epoch, offset);
        if period_start > current_epoch {
            return Err(actor_error!(
                illegal_state,
                "computed proving period start {} after current epoch {}",
                period_start,
                current_epoch
            ));
        }

        let deadline_idx = current_deadline_index(policy, current_epoch, period_start);
        if deadline_idx >= policy.wpost_period_deadlines {
            return Err(actor_error!(
                illegal_state,
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
        )?;
        let info_cid = rt.store().put_cbor(&info, Blake2b256).map_err(|e| {
            e.downcast_default(
                ExitCode::USR_ILLEGAL_STATE,
                "failed to construct illegal state",
            )
        })?;

        let st =
            State::new(policy, rt.store(), info_cid, period_start, deadline_idx).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to construct state")
            })?;
        rt.create(&st)?;

        Ok(())
    }

    fn control_addresses<BS, RT>(rt: &mut RT) -> Result<GetControlAddressesReturn, ActorError>
    where
        BS: Blockstore,
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
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        check_control_addresses(rt.policy(), &params.new_control_addresses)?;

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
                    effective_at: rt.curr_epoch() + rt.policy().worker_key_change_delay,
                })
            }

            state.save_info(rt.store(), &info).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "could not save miner info")
            })?;

            Ok(())
        })?;

        Ok(())
    }

    /// Triggers a worker address change if a change has been requested and its effective epoch has arrived.
    fn confirm_update_worker_key<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.transaction(|state: &mut State, rt| {
            let mut info = get_miner_info(rt.store(), state)?;

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
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        // * Cannot match go checking for undef address, does go impl allow this to be
        // * deserialized over the wire? If so, a workaround will be needed

        if !matches!(new_address.protocol(), Protocol::ID) {
            return Err(actor_error!(
                illegal_argument,
                "owner address must be an ID address"
            ));
        }

        rt.transaction(|state: &mut State, rt| {
            let mut info = get_miner_info(rt.store(), state)?;

            if rt.message().caller() == info.owner || info.pending_owner_address.is_none() {
                rt.validate_immediate_caller_is(std::iter::once(&info.owner))?;
                info.pending_owner_address = Some(new_address);
            } else {
                let pending_address = info.pending_owner_address.unwrap();
                rt.validate_immediate_caller_is(std::iter::once(&pending_address))?;
                if new_address != pending_address {
                    return Err(actor_error!(
                        illegal_argument,
                        "expected confirmation of {} got {}",
                        pending_address,
                        new_address
                    ));
                }

                // Change beneficiary address to new owner if current beneficiary address equal to old owner address
                if info.beneficiary == info.owner {
                    info.beneficiary = pending_address;
                }
                // Cancel pending beneficiary term change when the owner changes
                info.pending_beneficiary_term = None;

                // Set the new owner address
                info.owner = pending_address;
            }

            // Clear any no-op change
            if let Some(p_addr) = info.pending_owner_address {
                if p_addr == info.owner {
                    info.pending_owner_address = None;
                }
            }

            state.save_info(rt.store(), &info).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to save miner info")
            })?;

            Ok(())
        })
    }

    fn change_peer_id<BS, RT>(rt: &mut RT, params: ChangePeerIDParams) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let policy = rt.policy();
        check_peer_info(policy, &params.new_id, &[])?;

        rt.transaction(|state: &mut State, rt| {
            let mut info = get_miner_info(rt.store(), state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            info.peer_id = params.new_id;
            state.save_info(rt.store(), &info).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "could not save miner info")
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
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let policy = rt.policy();
        check_peer_info(policy, &[], &params.new_multi_addrs)?;

        rt.transaction(|state: &mut State, rt| {
            let mut info = get_miner_info(rt.store(), state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            info.multi_address = params.new_multi_addrs;
            state.save_info(rt.store(), &info).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "could not save miner info")
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
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let current_epoch = rt.curr_epoch();

        {
            let policy = rt.policy();
            if params.proofs.len() != 1 {
                return Err(actor_error!(
                    illegal_argument,
                    "expected exactly one proof, got {}",
                    params.proofs.len()
                ));
            }

            if check_valid_post_proof_type(policy, params.proofs[0].post_proof).is_err() {
                return Err(actor_error!(
                    illegal_argument,
                    "proof type {:?} not allowed",
                    params.proofs[0].post_proof
                ));
            }

            if params.deadline >= policy.wpost_period_deadlines {
                return Err(actor_error!(
                    illegal_argument,
                    "invalid deadline {} of {}",
                    params.deadline,
                    policy.wpost_period_deadlines
                ));
            }

            if params.chain_commit_rand.0.len() > RANDOMNESS_LENGTH {
                return Err(actor_error!(
                    illegal_argument,
                    "expected at most {} bytes of randomness, got {}",
                    RANDOMNESS_LENGTH,
                    params.chain_commit_rand.0.len()
                ));
            }
        }

        let post_result = rt.transaction(|state: &mut State, rt| {
            let info = get_miner_info(rt.store(), state)?;

            let max_proof_size = info.window_post_proof_type.proof_size().map_err(|e| {
                actor_error!(
                    illegal_state,
                    "failed to determine max window post proof size: {}",
                    e
                )
            })?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            // Verify that the miner has passed exactly 1 proof.
            if params.proofs.len() != 1 {
                return Err(actor_error!(
                    illegal_argument,
                    "expected exactly one proof, got {}",
                    params.proofs.len()
                ));
            }

            // Make sure the miner is using the correct proof type.
            if params.proofs[0].post_proof != info.window_post_proof_type {
                return Err(actor_error!(
                    illegal_argument,
                    "expected proof of type {:?}, got {:?}",
                    params.proofs[0].post_proof,
                    info.window_post_proof_type
                ));
            }

            // Make sure the proof size doesn't exceed the max. We could probably check for an exact match, but this is safer.
            let max_size = max_proof_size * params.partitions.len();
            if params.proofs[0].proof_bytes.len() > max_size {
                return Err(actor_error!(
                    illegal_argument,
                    "expected proof to be smaller than {} bytes",
                    max_size
                ));
            }

            // Validate that the miner didn't try to prove too many partitions at once.
            let submission_partition_limit =
                load_partitions_sectors_max(rt.policy(), info.window_post_partition_sectors);
            if params.partitions.len() as u64 > submission_partition_limit {
                return Err(actor_error!(
                    illegal_argument,
                    "too many partitions {}, limit {}",
                    params.partitions.len(),
                    submission_partition_limit
                ));
            }
            let current_deadline = state.deadline_info(rt.policy(), current_epoch);

            // Check that the miner state indicates that the current proving deadline has started.
            // This should only fail if the cron actor wasn't invoked, and matters only in case that it hasn't been
            // invoked for a whole proving period, and hence the missed PoSt submissions from the prior occurrence
            // of this deadline haven't been processed yet.
            if !current_deadline.is_open() {
                return Err(actor_error!(
                    illegal_state,
                    "proving period {} not yet open at {}",
                    current_deadline.period_start,
                    current_epoch
                ));
            }

            // The miner may only submit a proof for the current deadline.
            if params.deadline != current_deadline.index {
                return Err(actor_error!(
                    illegal_argument,
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
                    illegal_argument,
                    "expected chain commit epoch {} to be after {}",
                    params.chain_commit_epoch,
                    current_deadline.challenge
                ));
            }

            if params.chain_commit_epoch >= current_epoch {
                return Err(actor_error!(
                    illegal_argument,
                    "chain commit epoch {} must be less than the current epoch {}",
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
            if Randomness(comm_rand.into()) != params.chain_commit_rand {
                return Err(actor_error!(
                    illegal_argument,
                    "post commit randomness mismatched"
                ));
            }

            let sectors = Sectors::load(rt.store(), &state.sectors).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to load sectors")
            })?;

            let mut deadlines = state
                .load_deadlines(rt.store())
                .map_err(|e| e.wrap("failed to load deadlines"))?;

            let mut deadline = deadlines
                .load_deadline(rt.policy(), rt.store(), params.deadline)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::USR_ILLEGAL_STATE,
                        format!("failed to load deadline {}", params.deadline),
                    )
                })?;

            // Record proven sectors/partitions, returning updates to power and the final set of sectors
            // proven/skipped.
            //
            // NOTE: This function does not actually check the proofs but does assume that they're correct. Instead,
            // it snapshots the deadline's state and the submitted proofs at the end of the challenge window and
            // allows third-parties to dispute these proofs.
            //
            // While we could perform _all_ operations at the end of challenge window, we do as we can here to avoid
            // overloading cron.
            let policy = rt.policy();
            let fault_expiration = current_deadline.last() + policy.fault_max_age;
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
                        ExitCode::USR_ILLEGAL_STATE,
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
                    illegal_argument,
                    "cannot prove partitions with no active sectors"
                ));
            }
            // If we're not recovering power, record the proof for optimistic verification.
            if post_result.recovered_power.is_zero() {
                deadline
                    .record_post_proofs(rt.store(), &post_result.partitions, &params.proofs)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
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
                            ExitCode::USR_ILLEGAL_STATE,
                            "failed to load sectors for post verification",
                        )
                    })?;
                if !verify_windowed_post(
                    rt,
                    current_deadline.challenge,
                    &sector_infos,
                    params.proofs,
                )
                .map_err(|e| e.wrap("window post failed"))?
                {
                    return Err(actor_error!(illegal_argument, "invalid post was submitted"));
                }
            }

            let deadline_idx = params.deadline;
            deadlines
                .update_deadline(policy, rt.store(), params.deadline, &deadline)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::USR_ILLEGAL_STATE,
                        format!("failed to update deadline {}", deadline_idx),
                    )
                })?;

            state.save_deadlines(rt.store(), deadlines).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to save deadlines")
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
            .check_balance_invariants(&rt.current_balance())
            .map_err(balance_invariants_broken)?;

        Ok(())
    }
    /// Checks state of the corresponding sector pre-commitments and verifies aggregate proof of replication
    /// of these sectors. If valid, the sectors' deals are activated, sectors are assigned a deadline and charged pledge
    /// and precommit state is removed.
    fn prove_commit_aggregate<BS, RT>(
        rt: &mut RT,
        params: ProveCommitAggregateParams,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let sector_numbers = params.sector_numbers.validate().map_err(|e| {
            actor_error!(
                illegal_state,
                "Failed to validate bitfield for aggregated sectors: {}",
                e
            )
        })?;
        let agg_sectors_count = sector_numbers.len();

        {
            let policy = rt.policy();
            if agg_sectors_count > policy.max_aggregated_sectors {
                return Err(actor_error!(
                    illegal_argument,
                    "too many sectors addressed, addressed {} want <= {}",
                    agg_sectors_count,
                    policy.max_aggregated_sectors
                ));
            } else if agg_sectors_count < policy.min_aggregated_sectors {
                return Err(actor_error!(
                    illegal_argument,
                    "too few sectors addressed, addressed {} want >= {}",
                    agg_sectors_count,
                    policy.min_aggregated_sectors
                ));
            }

            if params.aggregate_proof.len() > policy.max_aggregated_proof_size {
                return Err(actor_error!(
                    illegal_argument,
                    "sector prove-commit proof of size {} exceeds max size of {}",
                    params.aggregate_proof.len(),
                    policy.max_aggregated_proof_size
                ));
            }
        }
        let state: State = rt.state()?;
        let info = get_miner_info(rt.store(), &state)?;
        rt.validate_immediate_caller_is(
            info.control_addresses
                .iter()
                .chain(&[info.worker, info.owner]),
        )?;
        let store = rt.store();
        let precommits = state
            .get_all_precommitted_sectors(store, sector_numbers)
            .map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to get precommits")
            })?;

        // validate each precommit
        let mut precommits_to_confirm = Vec::new();
        for (i, precommit) in precommits.iter().enumerate() {
            let msd = max_prove_commit_duration(rt.policy(), precommit.info.seal_proof)
                .ok_or_else(|| {
                    actor_error!(
                        illegal_state,
                        "no max seal duration for proof type: {}",
                        i64::from(precommit.info.seal_proof)
                    )
                })?;
            let prove_commit_due = precommit.pre_commit_epoch + msd;
            if rt.curr_epoch() > prove_commit_due {
                log::warn!(
                    "skipping commitment for sector {}, too late at {}, due {}",
                    precommit.info.sector_number,
                    rt.curr_epoch(),
                    prove_commit_due,
                )
            } else {
                precommits_to_confirm.push(precommit.clone());
            }
            // All seal proof types should match
            if i >= 1 {
                let prev_seal_proof = precommits[i - 1].info.seal_proof;
                if prev_seal_proof != precommit.info.seal_proof {
                    return Err(actor_error!(
                        illegal_state,
                        "aggregate contains mismatched seal proofs {} and {}",
                        i64::from(prev_seal_proof),
                        i64::from(precommit.info.seal_proof)
                    ));
                }
            }
        }
        let mut svis = Vec::with_capacity(precommits.len());
        let miner_actor_id: u64 = if let Payload::ID(i) = rt.message().receiver().payload() {
            *i
        } else {
            return Err(actor_error!(
                illegal_state,
                "runtime provided non-ID receiver address {}",
                rt.message().receiver()
            ));
        };
        let receiver_bytes = serialize_vec(
            &rt.message().receiver(),
            "address for seal verification challenge",
        )?;

        for precommit in precommits.iter() {
            let interactive_epoch =
                precommit.pre_commit_epoch + rt.policy().pre_commit_challenge_delay;
            if rt.curr_epoch() <= interactive_epoch {
                return Err(actor_error!(
                    forbidden,
                    "too early to prove sector {}",
                    precommit.info.sector_number
                ));
            }
            let sv_info_randomness = rt.get_randomness_from_tickets(
                DomainSeparationTag::SealRandomness,
                precommit.info.seal_rand_epoch,
                &receiver_bytes,
            )?;
            let sv_info_interactive_randomness = rt.get_randomness_from_beacon(
                DomainSeparationTag::InteractiveSealChallengeSeed,
                interactive_epoch,
                &receiver_bytes,
            )?;

            let unsealed_cid = precommit
                .info
                .unsealed_cid
                .get_cid(precommit.info.seal_proof)?;

            let svi = AggregateSealVerifyInfo {
                sector_number: precommit.info.sector_number,
                randomness: Randomness(sv_info_randomness.into()),
                interactive_randomness: Randomness(sv_info_interactive_randomness.into()),
                sealed_cid: precommit.info.sealed_cid,
                unsealed_cid,
            };
            svis.push(svi);
        }

        let seal_proof = precommits[0].info.seal_proof;
        if precommits.is_empty() {
            return Err(actor_error!(
                illegal_state,
                "bitfield non-empty but zero precommits read from state"
            ));
        }
        rt.verify_aggregate_seals(&AggregateSealVerifyProofAndInfos {
            miner: miner_actor_id,
            seal_proof,
            aggregate_proof: RegisteredAggregateProof::SnarkPackV2,
            proof: params.aggregate_proof,
            infos: svis,
        })
        .map_err(|e| {
            e.downcast_default(
                ExitCode::USR_ILLEGAL_ARGUMENT,
                "aggregate seal verify failed",
            )
        })?;

        let rew = request_current_epoch_block_reward(rt)?;
        let pwr = request_current_total_power(rt)?;
        confirm_sector_proofs_valid_internal(
            rt,
            precommits_to_confirm.clone(),
            &rew.this_epoch_baseline_power,
            &rew.this_epoch_reward_smoothed,
            &pwr.quality_adj_power_smoothed,
        )?;

        // Compute and burn the aggregate network fee. We need to re-load the state as
        // confirmSectorProofsValid can change it.
        let state: State = rt.state()?;
        let aggregate_fee =
            aggregate_prove_commit_network_fee(precommits_to_confirm.len() as i64, &rt.base_fee());
        let unlocked_balance = state
            .get_unlocked_balance(&rt.current_balance())
            .map_err(|_e| actor_error!(illegal_state, "failed to determine unlocked balance"))?;
        if unlocked_balance < aggregate_fee {
            return Err(actor_error!(
                insufficient_funds,
                "remaining unlocked funds after prove-commit {} are insufficient to pay aggregation fee of {}",
                unlocked_balance,
                aggregate_fee
            ));
        }
        burn_funds(rt, aggregate_fee)?;
        state
            .check_balance_invariants(&rt.current_balance())
            .map_err(balance_invariants_broken)?;
        Ok(())
    }

    fn prove_replica_updates<BS, RT>(
        rt: &mut RT,
        params: ProveReplicaUpdatesParams,
    ) -> Result<BitField, ActorError>
    where
        // + Clone because we messed up and need to keep a copy around between transactions.
        // https://github.com/filecoin-project/builtin-actors/issues/133
        BS: Blockstore + Clone,
        RT: Runtime<BS>,
    {
        // In this entry point, the unsealed CID is computed from deals via the market actor.
        // A future entry point will take the unsealed CID as parameter
        let updates = params
            .updates
            .into_iter()
            .map(|ru| ReplicaUpdateInner {
                sector_number: ru.sector_number,
                deadline: ru.deadline,
                partition: ru.partition,
                new_sealed_cid: ru.new_sealed_cid,
                new_unsealed_cid: None,
                deals: ru.deals,
                update_proof_type: ru.update_proof_type,
                replica_proof: ru.replica_proof,
            })
            .collect();
        Self::prove_replica_updates_inner(rt, updates)
    }

    fn prove_replica_updates2<BS, RT>(
        rt: &mut RT,
        params: ProveReplicaUpdatesParams2,
    ) -> Result<BitField, ActorError>
    where
        // + Clone because we messed up and need to keep a copy around between transactions.
        // https://github.com/filecoin-project/builtin-actors/issues/133
        BS: Blockstore + Clone,
        RT: Runtime<BS>,
    {
        let updates = params
            .updates
            .into_iter()
            .map(|ru| ReplicaUpdateInner {
                sector_number: ru.sector_number,
                deadline: ru.deadline,
                partition: ru.partition,
                new_sealed_cid: ru.new_sealed_cid,
                new_unsealed_cid: Some(ru.new_unsealed_cid),
                deals: ru.deals,
                update_proof_type: ru.update_proof_type,
                replica_proof: ru.replica_proof,
            })
            .collect();
        Self::prove_replica_updates_inner(rt, updates)
    }
    fn prove_replica_updates_inner<BS, RT>(
        rt: &mut RT,
        updates: Vec<ReplicaUpdateInner>,
    ) -> Result<BitField, ActorError>
    where
        // + Clone because we messed up and need to keep a copy around between transactions.
        // https://github.com/filecoin-project/builtin-actors/issues/133
        BS: Blockstore + Clone,
        RT: Runtime<BS>,
    {
        // Validate inputs

        if updates.len() > rt.policy().prove_replica_updates_max_size {
            return Err(actor_error!(
                illegal_argument,
                "too many updates ({} > {})",
                updates.len(),
                rt.policy().prove_replica_updates_max_size
            ));
        }

        let state: State = rt.state()?;
        let info = get_miner_info(rt.store(), &state)?;

        rt.validate_immediate_caller_is(
            info.control_addresses
                .iter()
                .chain(&[info.owner, info.worker]),
        )?;

        let sector_store = rt.store().clone();
        let mut sectors = Sectors::load(&sector_store, &state.sectors).map_err(|e| {
            e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to load sectors array")
        })?;

        let mut power_delta = PowerPair::zero();
        let mut pledge_delta = TokenAmount::zero();

        struct UpdateAndSectorInfo<'a> {
            update: &'a ReplicaUpdateInner,
            sector_info: SectorOnChainInfo,
            deal_spaces: ext::market::DealSpaces,
        }

        let mut sectors_deals = Vec::<ext::market::SectorDeals>::new();
        let mut sectors_data_spec = Vec::<ext::market::SectorDataSpec>::new();
        let mut validated_updates = Vec::<UpdateAndSectorInfo>::new();
        let mut sector_numbers = BitField::new();
        for update in updates.iter() {
            let set = sector_numbers.get(update.sector_number);
            if set {
                info!(
                    "duplicate sector being updated {}, skipping",
                    update.sector_number,
                );
                continue;
            }

            if sector_numbers.try_set(update.sector_number).is_err() {
                info!("invalid sector number, skipping");
                continue;
            }

            if update.replica_proof.len() > 4096 {
                info!(
                    "update proof is too large ({}), skipping sector {}",
                    update.replica_proof.len(),
                    update.sector_number,
                );
                continue;
            }

            if update.deals.is_empty() {
                info!(
                    "must have deals to update, skipping sector {}",
                    update.sector_number,
                );
                continue;
            }

            if update.deals.len() as u64 > sector_deals_max(rt.policy(), info.sector_size) {
                info!(
                    "more deals than policy allows, skipping sector {}",
                    update.sector_number,
                );
                continue;
            }

            if update.deadline >= rt.policy().wpost_period_deadlines {
                info!(
                    "deadline {} not in range 0..{}, skipping sector {}",
                    update.deadline,
                    rt.policy().wpost_period_deadlines,
                    update.sector_number
                );
                continue;
            }

            // Skip checking if CID is defined because it cannot be so in Rust

            if !is_sealed_sector(&update.new_sealed_cid) {
                info!(
                    "new sealed CID had wrong prefix {}, skipping sector {}",
                    update.new_sealed_cid, update.sector_number
                );
                continue;
            }

            // If the deadline is the current or next deadline to prove, don't allow updating sectors.
            // We assume that deadlines are immutable when being proven.
            if !deadline_is_mutable(
                rt.policy(),
                state.current_proving_period_start(rt.policy(), rt.curr_epoch()),
                update.deadline,
                rt.curr_epoch(),
            ) {
                info!(
                    "cannot upgrade sectors in immutable deadline {}, skipping sector {}",
                    update.deadline, update.sector_number
                );
                continue;
            }

            if !state
                .check_sector_active(
                    rt.policy(),
                    rt.store(),
                    update.deadline,
                    update.partition,
                    update.sector_number,
                    true,
                )
                .map_err(|_| actor_error!(illegal_argument, "error checking sector health"))?
            {
                info!(
                    "sector isn't healthy, skipping sector {}",
                    update.sector_number
                );
                continue;
            }

            let res = Sectors::must_get(&sectors, update.sector_number);
            let sector_info = if let Ok(value) = res {
                value
            } else {
                info!(
                    "failed to get sector, skipping sector {}",
                    update.sector_number
                );
                continue;
            };

            if !sector_info.deal_ids.is_empty() {
                info!(
                    "cannot update sector with deals, skipping sector {}",
                    update.sector_number
                );
                continue;
            }

            let deal_spaces = match activate_deals_and_claim_allocations(
                rt,
                update.deals.clone(),
                sector_info.expiration,
                sector_info.sector_number,
            )? {
                Some(deal_spaces) => deal_spaces,
                None => {
                    info!(
                        "failed to activate deals on sector {}, skipping from replica update set",
                        update.sector_number
                    );
                    continue;
                }
            };

            let expiration = sector_info.expiration;
            let seal_proof = sector_info.seal_proof;
            validated_updates.push(UpdateAndSectorInfo {
                update,
                sector_info,
                deal_spaces,
            });

            sectors_deals.push(ext::market::SectorDeals {
                sector_type: seal_proof,
                deal_ids: update.deals.clone(),
                sector_expiry: expiration,
            });
            sectors_data_spec.push(ext::market::SectorDataSpec {
                sector_type: seal_proof,
                deal_ids: update.deals.clone(),
            });
        }

        if validated_updates.is_empty() {
            return Err(actor_error!(illegal_argument, "no valid updates"));
        }

        // Errors past this point cause the prove_replica_updates call to fail (no more skipping sectors)

        let deal_data = request_deal_data(rt, &sectors_deals)?;
        if deal_data.sectors.len() != validated_updates.len() {
            return Err(actor_error!(
                illegal_state,
                "deal weight request returned {} records, expected {}",
                deal_data.sectors.len(),
                validated_updates.len()
            ));
        }

        struct UpdateWithDetails<'a> {
            update: &'a ReplicaUpdateInner,
            sector_info: &'a SectorOnChainInfo,
            deal_spaces: &'a ext::market::DealSpaces,
            full_unsealed_cid: Cid,
        }

        // Group declarations by deadline
        let mut decls_by_deadline = BTreeMap::<u64, Vec<UpdateWithDetails>>::new();
        let mut deadlines_to_load = Vec::<u64>::new();
        for (with_sector_info, deal_data) in validated_updates.iter().zip(deal_data.sectors.iter())
        {
            let computed_commd = CompactCommD::new(deal_data.commd)
                .get_cid(with_sector_info.sector_info.seal_proof)?;
            if let Some(ref declared_commd) = with_sector_info.update.new_unsealed_cid {
                if !declared_commd.eq(&computed_commd) {
                    info!(
                        "unsealed CID does not match with deals: expected {}, got {}, sector: {}",
                        computed_commd, declared_commd, with_sector_info.update.sector_number
                    );
                }
            }
            let dl = with_sector_info.update.deadline;
            if !decls_by_deadline.contains_key(&dl) {
                deadlines_to_load.push(dl);
            }

            decls_by_deadline
                .entry(dl)
                .or_default()
                .push(UpdateWithDetails {
                    update: with_sector_info.update,
                    sector_info: &with_sector_info.sector_info,
                    deal_spaces: &with_sector_info.deal_spaces,
                    full_unsealed_cid: computed_commd,
                });
        }

        let rew = request_current_epoch_block_reward(rt)?;
        let pow = request_current_total_power(rt)?;

        let succeeded_sectors = rt.transaction(|state: &mut State, rt| {
            let mut succeeded = Vec::new();
            let mut deadlines = state
                .load_deadlines(rt.store())?;

            let mut new_sectors = Vec::with_capacity(validated_updates.len());
            for &dl_idx in deadlines_to_load.iter() {
                let mut deadline = deadlines
                    .load_deadline(rt.policy(), rt.store(), dl_idx)
                    .map_err(|e|
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            format!("failed to load deadline {}", dl_idx),
                        )
                    )?;

                let mut partitions = deadline
                    .partitions_amt(rt.store())
                    .map_err(|e|
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            format!("failed to load partitions for deadline {}", dl_idx),
                        )
                    )?;

                let quant = state.quant_spec_for_deadline(rt.policy(), dl_idx);

                for with_details in &decls_by_deadline[&dl_idx] {
                    let update_proof_type = with_details.sector_info.seal_proof
                        .registered_update_proof()
                        .map_err(|_|
                            actor_error!(
                                illegal_state,
                                "couldn't load update proof type"
                            )
                        )?;
                    if with_details.update.update_proof_type != update_proof_type {
                        return Err(actor_error!(
                            illegal_argument,
                            format!("unsupported update proof type {}", i64::from(with_details.update.update_proof_type))
                        ));
                    }

                    rt.verify_replica_update(
                        &ReplicaUpdateInfo {
                            update_proof_type,
                            new_sealed_cid: with_details.update.new_sealed_cid,
                            old_sealed_cid: with_details.sector_info.sealed_cid,
                            new_unsealed_cid: with_details.full_unsealed_cid,
                            proof: with_details.update.replica_proof.clone(),
                        }
                    )
                        .map_err(|e|
                            e.downcast_default(
                                ExitCode::USR_ILLEGAL_ARGUMENT,
                                format!("failed to verify replica proof for sector {}", with_details.sector_info.sector_number),
                            )
                        )?;

                    let mut new_sector_info = with_details.sector_info.clone();

                    new_sector_info.simple_qa_power = true;
                    new_sector_info.sealed_cid = with_details.update.new_sealed_cid;
                    new_sector_info.sector_key_cid = match new_sector_info.sector_key_cid {
                        None => Some(with_details.sector_info.sealed_cid),
                        Some(x) => Some(x),
                    };
                    // Skip checking if CID is defined because it cannot be so in Rust

                    new_sector_info.deal_ids = with_details.update.deals.clone();
                    new_sector_info.activation = rt.curr_epoch();

                    let duration = new_sector_info.expiration - new_sector_info.activation;

                    new_sector_info.deal_weight = with_details.deal_spaces.deal_space.clone() * duration;
                    new_sector_info.verified_deal_weight = with_details.deal_spaces.verified_deal_space.clone() * duration;

                    // compute initial pledge
                    let qa_pow = qa_power_for_weight(
                        info.sector_size,
                        duration,
                        &new_sector_info.deal_weight,
                        &new_sector_info.verified_deal_weight,
                    );

                    new_sector_info.replaced_day_reward = with_details.sector_info.expected_day_reward.clone();
                    new_sector_info.expected_day_reward = expected_reward_for_power(
                        &rew.this_epoch_reward_smoothed,
                        &pow.quality_adj_power_smoothed,
                        &qa_pow,
                        fil_actors_runtime_v9::network::EPOCHS_IN_DAY,
                    );
                    new_sector_info.expected_storage_pledge = expected_reward_for_power(
                        &rew.this_epoch_reward_smoothed,
                        &pow.quality_adj_power_smoothed,
                        &qa_pow,
                        INITIAL_PLEDGE_PROJECTION_PERIOD,
                    );
                    new_sector_info.replaced_sector_age =
                        ChainEpoch::max(0, rt.curr_epoch() - with_details.sector_info.activation);

                    let initial_pledge_at_upgrade = initial_pledge_for_power(
                        &qa_pow,
                        &rew.this_epoch_baseline_power,
                        &rew.this_epoch_reward_smoothed,
                        &pow.quality_adj_power_smoothed,
                        &rt.total_fil_circ_supply(),
                    );

                    if initial_pledge_at_upgrade > with_details.sector_info.initial_pledge {
                        let deficit = &initial_pledge_at_upgrade - &with_details.sector_info.initial_pledge;

                        let unlocked_balance = state
                            .get_unlocked_balance(&rt.current_balance())
                            .map_err(|_|
                                actor_error!(illegal_state, "failed to calculate unlocked balance")
                            )?;
                        if unlocked_balance < deficit {
                            return Err(actor_error!(
                                insufficient_funds,
                                "insufficient funds for new initial pledge requirement {}, available: {}, skipping sector {}",
                                deficit,
                                unlocked_balance,
                                with_details.sector_info.sector_number
                            ));
                        }

                        state.add_initial_pledge(&deficit).map_err(|_e|
                            actor_error!(
                                illegal_state,
                                "failed to add initial pledge"
                            )
                        )?;

                        new_sector_info.initial_pledge = initial_pledge_at_upgrade;
                    }

                    let mut partition = partitions
                        .get(with_details.update.partition)
                        .map_err(|e|
                            e.downcast_default(
                                ExitCode::USR_ILLEGAL_STATE,
                                format!("failed to load deadline {} partition {}", with_details.update.deadline, with_details.update.partition),
                            )
                        )?
                        .cloned()
                        .ok_or_else(|| actor_error!(not_found, "no such deadline {} partition {}", dl_idx, with_details.update.partition))?;

                    let (partition_power_delta, partition_pledge_delta) = partition
                        .replace_sectors(rt.store(),
                                         &[with_details.sector_info.clone()],
                                         &[new_sector_info.clone()],
                                         info.sector_size,
                                         quant,
                        )
                        .map_err(|e| {
                            e.downcast_default(
                                ExitCode::USR_ILLEGAL_STATE,
                                format!("failed to replace sector at deadline {} partition {}", with_details.update.deadline, with_details.update.partition),
                            )
                        })?;

                    power_delta += &partition_power_delta;
                    pledge_delta += &partition_pledge_delta;

                    partitions
                        .set(with_details.update.partition, partition)
                        .map_err(|e| {
                            e.downcast_default(
                                ExitCode::USR_ILLEGAL_STATE,
                                format!("failed to save deadline {} partition {}", with_details.update.deadline, with_details.update.partition),
                            )
                        })?;

                    succeeded.push(new_sector_info.sector_number);
                    new_sectors.push(new_sector_info);
                }

                deadline.partitions = partitions.flush().map_err(|e| {
                    e.downcast_default(
                        ExitCode::USR_ILLEGAL_STATE,
                        format!("failed to save partitions for deadline {}", dl_idx),
                    )
                })?;

                deadlines
                    .update_deadline(rt.policy(), rt.store(), dl_idx, &deadline)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            format!("failed to save deadline {}", dl_idx),
                        )
                    })?;
            }

            let success_len = succeeded.len();
            if success_len != validated_updates.len() {
                return Err(actor_error!(
                    illegal_state,
                    "unexpected success_len {} != {}",
                    success_len,
                    validated_updates.len()
                ));
            }
            if new_sectors.len() != validated_updates.len() {
                return Err(actor_error!(
                    illegal_state,
                    "unexpected new_sectors len {} != {}",
                    new_sectors.len(),
                    validated_updates.len()
                ));
            }

            // Overwrite sector infos.
            sectors.store(new_sectors).map_err(|e| {
                e.downcast_default(
                    ExitCode::USR_ILLEGAL_STATE,
                    "failed to update sector infos",
                )
            })?;

            state.sectors = sectors.amt.flush().map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to save sectors")
            })?;
            state.save_deadlines(rt.store(), deadlines).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to save deadlines")
            })?;

            BitField::try_from_bits(succeeded).map_err(|_| {
                actor_error!(illegal_argument; "invalid sector number")
            })
        })?;

        notify_pledge_changed(rt, &pledge_delta)?;
        request_update_power(rt, power_delta)?;

        Ok(succeeded_sectors)
    }

    fn dispute_windowed_post<BS, RT>(
        rt: &mut RT,
        params: DisputeWindowedPoStParams,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;
        let reporter = rt.message().caller();

        {
            let policy = rt.policy();
            if params.deadline >= policy.wpost_period_deadlines {
                return Err(actor_error!(
                    illegal_argument,
                    "invalid deadline {} of {}",
                    params.deadline,
                    policy.wpost_period_deadlines
                ));
            }
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
                let policy = rt.policy();
                let dl_info = st.deadline_info(policy, current_epoch);

                if !deadline_available_for_optimistic_post_dispute(
                    policy,
                    dl_info.period_start,
                    params.deadline,
                    current_epoch,
                ) {
                    return Err(actor_error!(
                        forbidden,
                        "can only dispute window posts during the dispute window\
                    ({} epochs after the challenge window closes)",
                        policy.wpost_dispute_window
                    ));
                }

                let info = get_miner_info(rt.store(), st)?;
                // --- check proof ---

                // Find the proving period start for the deadline in question.
                let mut pp_start = dl_info.period_start;
                if dl_info.index < params.deadline as u64 {
                    pp_start -= policy.wpost_proving_period
                }
                let target_deadline =
                    new_deadline_info(policy, pp_start, params.deadline, current_epoch);
                // Load the target deadline
                let mut deadlines_current = st
                    .load_deadlines(rt.store())
                    .map_err(|e| e.wrap("failed to load deadlines"))?;

                let mut dl_current = deadlines_current
                    .load_deadline(policy, rt.store(), params.deadline)
                    .map_err(|e| {
                        e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to load deadline")
                    })?;

                // Take the post from the snapshot for dispute.
                // This operation REMOVES the PoSt from the snapshot so
                // it can't be disputed again. If this method fails,
                // this operation must be rolled back.
                let (partitions, proofs) = dl_current
                    .take_post_proofs(rt.store(), params.post_index)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            "failed to load proof for dispute",
                        )
                    })?;

                // Load the partition info we need for the dispute.
                let mut dispute_info = dl_current
                    .load_partitions_for_dispute(rt.store(), partitions)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            "failed to load partition for dispute",
                        )
                    })?;

                // This includes power that is no longer active (e.g., due to sector terminations).
                // It must only be used for penalty calculations, not power adjustments.
                let penalised_power = dispute_info.disputed_power.clone();

                // Load sectors for the dispute.
                let sectors =
                    Sectors::load(rt.store(), &dl_current.sectors_snapshot).map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            "failed to load sectors array",
                        )
                    })?;
                let sector_infos = sectors
                    .load_for_proof(
                        &dispute_info.all_sector_nos,
                        &dispute_info.ignored_sector_nos,
                    )
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            "failed to load sectors to dispute window post",
                        )
                    })?;

                // Check proof, we fail if validation succeeds.
                if verify_windowed_post(rt, target_deadline.challenge, &sector_infos, proofs)? {
                    return Err(actor_error!(
                        illegal_argument,
                        "failed to dispute valid post"
                    ));
                } else {
                    info!("Successfully disputed post- window post was invalid");
                }

                // Ok, now we record faults. This always works because
                // we don't allow compaction/moving sectors during the
                // challenge window.
                //
                // However, some of these sectors may have been
                // terminated. That's fine, we'll skip them.
                let fault_expiration_epoch = target_deadline.last() + policy.fault_max_age;
                let power_delta = dl_current
                    .record_faults(
                        rt.store(),
                        &sectors,
                        info.sector_size,
                        quant_spec_for_deadline(policy, &target_deadline),
                        fault_expiration_epoch,
                        &mut dispute_info.disputed_sectors,
                    )
                    .map_err(|e| {
                        e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to declare faults")
                    })?;

                deadlines_current
                    .update_deadline(policy, rt.store(), params.deadline, &dl_current)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            format!("failed to update deadline {}", params.deadline),
                        )
                    })?;

                st.save_deadlines(rt.store(), deadlines_current)
                    .map_err(|e| {
                        e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to save deadlines")
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
                    .map_err(|e| actor_error!(illegal_state, "failed to apply penalty {}", e))?;
                let (penalty_from_vesting, penalty_from_balance) = st
                    .repay_partial_debt_in_priority_order(
                        rt.store(),
                        current_epoch,
                        &rt.current_balance(),
                    )
                    .map_err(|e| {
                        e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to pay debt")
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
                &reporter,
                METHOD_SEND,
                RawBytes::default(),
                to_reward.clone(),
            ) {
                error!("failed to send reward: {}", e);
                to_burn += to_reward;
            }
        }

        burn_funds(rt, to_burn)?;
        notify_pledge_changed(rt, &pledge_delta)?;

        let st: State = rt.state()?;
        st.check_balance_invariants(&rt.current_balance())
            .map_err(balance_invariants_broken)?;
        Ok(())
    }

    /// Pledges to seal and commit a single sector.
    /// See PreCommitSectorBatch for details.
    fn pre_commit_sector<BS, RT>(
        rt: &mut RT,
        params: PreCommitSectorParams,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let batch_params = PreCommitSectorBatchParams {
            sectors: vec![params],
        };
        Self::pre_commit_sector_batch(rt, batch_params)
    }

    /// Pledges the miner to seal and commit some new sectors.
    /// The caller specifies sector numbers, sealed sector data CIDs, seal randomness epoch, expiration, and the IDs
    /// of any storage deals contained in the sector data. The storage deal proposals must be already submitted
    /// to the storage market actor.
    /// A pre-commitment may specify an existing committed-capacity sector that the committed sector will replace
    /// when proven.
    /// This method calculates the sector's power, locks a pre-commit deposit for the sector, stores information about the
    /// sector in state and waits for it to be proven or expire.
    fn pre_commit_sector_batch<BS, RT>(
        rt: &mut RT,
        params: PreCommitSectorBatchParams,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let sectors = params
            .sectors
            .into_iter()
            .map(|spci| {
                if spci.replace_capacity {
                    Err(actor_error!(
                        forbidden,
                        "cc upgrade through precommit discontinued, use ProveReplicaUpdate"
                    ))
                } else {
                    Ok(SectorPreCommitInfoInner {
                        seal_proof: spci.seal_proof,
                        sector_number: spci.sector_number,
                        sealed_cid: spci.sealed_cid,
                        seal_rand_epoch: spci.seal_rand_epoch,
                        deal_ids: spci.deal_ids,
                        expiration: spci.expiration,
                        // This entry point computes the unsealed CID from deals via the market.
                        // A future one will accept it directly as a parameter.
                        unsealed_cid: None,
                    })
                }
            })
            .collect::<Result<_, _>>()?;
        Self::pre_commit_sector_batch_inner(rt, sectors)
    }

    /// Pledges the miner to seal and commit some new sectors.
    /// The caller specifies sector numbers, sealed sector CIDs, unsealed sector CID, seal randomness epoch, expiration, and the IDs
    /// of any storage deals contained in the sector data. The storage deal proposals must be already submitted
    /// to the storage market actor.
    /// This method calculates the sector's power, locks a pre-commit deposit for the sector, stores information about the
    /// sector in state and waits for it to be proven or expire.
    fn pre_commit_sector_batch2<BS, RT>(
        rt: &mut RT,
        params: PreCommitSectorBatchParams2,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        Self::pre_commit_sector_batch_inner(
            rt,
            params
                .sectors
                .into_iter()
                .map(|spci| SectorPreCommitInfoInner {
                    seal_proof: spci.seal_proof,
                    sector_number: spci.sector_number,
                    sealed_cid: spci.sealed_cid,
                    seal_rand_epoch: spci.seal_rand_epoch,
                    deal_ids: spci.deal_ids,
                    expiration: spci.expiration,

                    unsealed_cid: Some(spci.unsealed_cid),
                })
                .collect(),
        )
    }

    /// This function combines old and new flows for PreCommit with use Option<CommpactCommD>
    /// The old PreCommits will call this with None, new ones with Some(CompactCommD).
    fn pre_commit_sector_batch_inner<BS, RT>(
        rt: &mut RT,
        sectors: Vec<SectorPreCommitInfoInner>,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let curr_epoch = rt.curr_epoch();
        {
            let policy = rt.policy();
            if sectors.is_empty() {
                return Err(actor_error!(illegal_argument, "batch empty"));
            } else if sectors.len() > policy.pre_commit_sector_batch_max_size {
                return Err(actor_error!(
                    illegal_argument,
                    "batch of {} too large, max {}",
                    sectors.len(),
                    policy.pre_commit_sector_batch_max_size
                ));
            }
        }
        // Check per-sector preconditions before opening state transaction or sending other messages.
        let challenge_earliest = curr_epoch - rt.policy().max_pre_commit_randomness_lookback;
        let mut sectors_deals = Vec::with_capacity(sectors.len());
        let mut sector_numbers = BitField::new();
        for precommit in sectors.iter() {
            let set = sector_numbers.get(precommit.sector_number);
            if set {
                return Err(actor_error!(
                    illegal_argument,
                    "duplicate sector number {}",
                    precommit.sector_number
                ));
            }
            sector_numbers.set(precommit.sector_number);

            if !can_pre_commit_seal_proof(rt.policy(), precommit.seal_proof) {
                return Err(actor_error!(
                    illegal_argument,
                    "unsupported seal proof type {}",
                    i64::from(precommit.seal_proof)
                ));
            }
            if precommit.sector_number > MAX_SECTOR_NUMBER {
                return Err(actor_error!(
                    illegal_argument,
                    "sector number {} out of range 0..(2^63-1)",
                    precommit.sector_number
                ));
            }
            // Skip checking if CID is defined because it cannot be so in Rust

            if !is_sealed_sector(&precommit.sealed_cid) {
                return Err(actor_error!(
                    illegal_argument,
                    "sealed CID had wrong prefix"
                ));
            }
            if precommit.seal_rand_epoch >= curr_epoch {
                return Err(actor_error!(
                    illegal_argument,
                    "seal challenge epoch {} must be before now {}",
                    precommit.seal_rand_epoch,
                    curr_epoch
                ));
            }
            if precommit.seal_rand_epoch < challenge_earliest {
                return Err(actor_error!(
                    illegal_argument,
                    "seal challenge epoch {} too old, must be after {}",
                    precommit.seal_rand_epoch,
                    challenge_earliest
                ));
            }

            if let Some(ref commd) = precommit.unsealed_cid.as_ref().and_then(|c| c.0) {
                if !is_unsealed_sector(commd) {
                    return Err(actor_error!(
                        illegal_argument,
                        "unsealed CID had wrong prefix"
                    ));
                }
            }

            // Require sector lifetime meets minimum by assuming activation happens at last epoch permitted for seal proof.
            // This could make sector maximum lifetime validation more lenient if the maximum sector limit isn't hit first.
            let max_activation = curr_epoch
                + max_prove_commit_duration(rt.policy(), precommit.seal_proof).unwrap_or_default();
            validate_expiration(
                rt.policy(),
                curr_epoch,
                max_activation,
                precommit.expiration,
                precommit.seal_proof,
            )?;

            sectors_deals.push(ext::market::SectorDeals {
                sector_type: precommit.seal_proof,
                sector_expiry: precommit.expiration,
                deal_ids: precommit.deal_ids.clone(),
            })
        }
        // gather information from other actors
        let reward_stats = request_current_epoch_block_reward(rt)?;
        let power_total = request_current_total_power(rt)?;
        let deal_data_vec = request_deal_data(rt, &sectors_deals)?;
        if deal_data_vec.sectors.len() != sectors.len() {
            return Err(actor_error!(
                illegal_state,
                "deal weight request returned {} records, expected {}",
                deal_data_vec.sectors.len(),
                sectors.len()
            ));
        }
        let mut fee_to_burn = TokenAmount::zero();
        let mut needs_cron = false;
        rt.transaction(|state: &mut State, rt| {
            // Aggregate fee applies only when batching.
            if sectors.len() > 1 {
                let aggregate_fee = aggregate_pre_commit_network_fee(sectors.len() as i64, &rt.base_fee());
                // AggregateFee applied to fee debt to consolidate burn with outstanding debts
                state.apply_penalty(&aggregate_fee)
                    .map_err(|e| {
                        actor_error!(
                        illegal_state,
                        "failed to apply penalty: {}",
                        e
                    )
                    })?;
            }
            // available balance already accounts for fee debt so it is correct to call
            // this before RepayDebts. We would have to
            // subtract fee debt explicitly if we called this after.
            let available_balance = state
                .get_available_balance(&rt.current_balance())
                .map_err(|e| {
                    actor_error!(
                        illegal_state,
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
            let store = rt.store();
            if consensus_fault_active(&info, curr_epoch) {
                return Err(actor_error!(forbidden, "pre-commit not allowed during active consensus fault"));
            }

            let mut chain_infos = Vec::with_capacity(sectors.len());
            let mut total_deposit_required = TokenAmount::zero();
            let mut clean_up_events = Vec::with_capacity(sectors.len());
            let deal_count_max = sector_deals_max(rt.policy(), info.sector_size);

            let sector_weight_for_deposit = qa_power_max(info.sector_size);
            let deposit_req = pre_commit_deposit_for_power(&reward_stats.this_epoch_reward_smoothed, &power_total.quality_adj_power_smoothed, &sector_weight_for_deposit);

            for (i, precommit) in sectors.into_iter().enumerate() {
                // Sector must have the same Window PoSt proof type as the miner's recorded seal type.
                let sector_wpost_proof = precommit.seal_proof
                    .registered_window_post_proof()
                    .map_err(|_e|
                        actor_error!(
                        illegal_argument,
                        "failed to lookup Window PoSt proof type for sector seal proof {}",
                        i64::from(precommit.seal_proof)
                    ))?;
                if sector_wpost_proof != info.window_post_proof_type {
                    return Err(actor_error!(illegal_argument, "sector Window PoSt proof type %d must match miner Window PoSt proof type {} (seal proof type {})", i64::from(sector_wpost_proof), i64::from(info.window_post_proof_type)));
                }
                if precommit.deal_ids.len() as u64 > deal_count_max {
                    return Err(actor_error!(illegal_argument, "too many deals for sector {} > {}", precommit.deal_ids.len(), deal_count_max));
                }

                let deal_data = &deal_data_vec.sectors[i];

                // 1. verify that precommit.unsealed_cid is correct
                // 2. create a new on_chain_precommit

                let commd = match precommit.unsealed_cid {
                    // if the CommD is unknown, use CommD computed by the market
                    None => CompactCommD::new(deal_data.commd),
                    Some(x) => x,
                };
                if commd.0 != deal_data.commd {
                    return Err(actor_error!(illegal_argument, "computed {:?} and passed {:?} CommDs not equal",
                            deal_data.commd, commd));
                }


                let on_chain_precommit = SectorPreCommitInfo {
                    seal_proof: precommit.seal_proof,
                    sector_number: precommit.sector_number,
                    sealed_cid: precommit.sealed_cid,
                    seal_rand_epoch: precommit.seal_rand_epoch,
                    deal_ids: precommit.deal_ids,
                    expiration: precommit.expiration,
                    unsealed_cid: commd,
                };

                // Build on-chain record.
                chain_infos.push(SectorPreCommitOnChainInfo {
                    info: on_chain_precommit,
                    pre_commit_deposit: deposit_req.clone(),
                    pre_commit_epoch: curr_epoch,
                });

                total_deposit_required += &deposit_req;

                // Calculate pre-commit cleanup
                let seal_proof = precommit.seal_proof;
                let msd = max_prove_commit_duration(rt.policy(), seal_proof)
                    .ok_or_else(|| actor_error!(illegal_argument, "no max seal duration set for proof type: {}", i64::from(seal_proof)))?;
                // PreCommitCleanUpDelay > 0 here is critical for the batch verification of proofs. Without it, if a proof arrived exactly on the
                // due epoch, ProveCommitSector would accept it, then the expiry event would remove it, and then
                // ConfirmSectorProofsValid would fail to find it.
                let clean_up_bound = curr_epoch + msd + rt.policy().expired_pre_commit_clean_up_delay;
                clean_up_events.push((clean_up_bound, precommit.sector_number));
            }
            // Batch update actor state.
            if available_balance < total_deposit_required {
                return Err(actor_error!(insufficient_funds, "insufficient funds {} for pre-commit deposit: {}", available_balance, total_deposit_required));
            }
            state.add_pre_commit_deposit(&total_deposit_required)
                .map_err(|e|
                    actor_error!(
                        illegal_state,
                        "failed to add pre-commit deposit {}: {}",
                        total_deposit_required, e
                ))?;
            state.allocate_sector_numbers(store, &sector_numbers, CollisionPolicy::DenyCollisions)
                .map_err(|e|
                    e.wrap("failed to allocate sector numbers")
                )?;
            state.put_precommitted_sectors(store, chain_infos)
                .map_err(|e|
                    e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to write pre-committed sectors")
                )?;
            state.add_pre_commit_clean_ups(rt.policy(), store, clean_up_events)
                .map_err(|e| {
                    e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to add pre-commit expiry to queue")
                })?;
            // Activate miner cron
            needs_cron = !state.deadline_cron_active;
            state.deadline_cron_active = true;
            Ok(())
        })?;
        burn_funds(rt, fee_to_burn)?;
        let state: State = rt.state()?;
        state
            .check_balance_invariants(&rt.current_balance())
            .map_err(balance_invariants_broken)?;
        if needs_cron {
            let new_dl_info = state.deadline_info(rt.policy(), curr_epoch);
            enroll_cron_event(
                rt,
                new_dl_info.last(),
                CronEventPayload {
                    event_type: CRON_EVENT_PROVING_DEADLINE,
                },
            )?;
        }
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
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;

        if params.sector_number > MAX_SECTOR_NUMBER {
            return Err(actor_error!(
                illegal_argument,
                "sector number greater than maximum"
            ));
        }

        let sector_number = params.sector_number;

        let st: State = rt.state()?;
        let precommit = st
            .get_precommitted_sector(rt.store(), sector_number)
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::USR_ILLEGAL_STATE,
                    format!("failed to load pre-committed sector {}", sector_number),
                )
            })?
            .ok_or_else(|| actor_error!(not_found, "no pre-commited sector {}", sector_number))?;

        let max_proof_size = precommit.info.seal_proof.proof_size().map_err(|e| {
            actor_error!(
                illegal_state,
                "failed to determine max proof size for sector {}: {}",
                sector_number,
                e
            )
        })?;
        if params.proof.len() > max_proof_size {
            return Err(actor_error!(
                illegal_argument,
                "sector prove-commit proof of size {} exceeds max size of {}",
                params.proof.len(),
                max_proof_size
            ));
        }

        let msd =
            max_prove_commit_duration(rt.policy(), precommit.info.seal_proof).ok_or_else(|| {
                actor_error!(
                    illegal_state,
                    "no max seal duration set for proof type: {:?}",
                    precommit.info.seal_proof
                )
            })?;
        let prove_commit_due = precommit.pre_commit_epoch + msd;
        if rt.curr_epoch() > prove_commit_due {
            return Err(actor_error!(
                illegal_argument,
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
                interactive_epoch: precommit.pre_commit_epoch
                    + rt.policy().pre_commit_challenge_delay,
                seal_rand_epoch: precommit.info.seal_rand_epoch,
                proof: params.proof,
                deal_ids: precommit.info.deal_ids.clone(),
                sector_num: precommit.info.sector_number,
                registered_seal_proof: precommit.info.seal_proof,
            },
            precommit.info.unsealed_cid,
        )?;

        rt.send(
            &STORAGE_POWER_ACTOR_ADDR,
            ext::power::SUBMIT_POREP_FOR_BULK_VERIFY_METHOD,
            RawBytes::serialize(&svi)?,
            TokenAmount::zero(),
        )?;

        Ok(())
    }

    fn confirm_sector_proofs_valid<BS, RT>(
        rt: &mut RT,
        params: ConfirmSectorProofsParams,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(iter::once(&STORAGE_POWER_ACTOR_ADDR))?;

        // This should be enforced by the power actor. We log here just in case
        // something goes wrong.
        if params.sectors.len() > ext::power::MAX_MINER_PROVE_COMMITS_PER_EPOCH {
            warn!(
                "confirmed more prove commits in an epoch than permitted: {} > {}",
                params.sectors.len(),
                ext::power::MAX_MINER_PROVE_COMMITS_PER_EPOCH
            );
        }
        let st: State = rt.state()?;
        let store = rt.store();
        // This skips missing pre-commits.
        let precommited_sectors = st
            .find_precommitted_sectors(store, &params.sectors)
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::USR_ILLEGAL_STATE,
                    "failed to load pre-committed sectors",
                )
            })?;
        confirm_sector_proofs_valid_internal(
            rt,
            precommited_sectors,
            &params.reward_baseline_power,
            &params.reward_smoothed,
            &params.quality_adj_power_smoothed,
        )
    }

    fn check_sector_proven<BS, RT>(
        rt: &mut RT,
        params: CheckSectorProvenParams,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;

        if params.sector_number > MAX_SECTOR_NUMBER {
            return Err(actor_error!(illegal_argument, "sector number out of range"));
        }

        let st: State = rt.state()?;

        match st.get_sector(rt.store(), params.sector_number) {
            Err(e) => Err(actor_error!(
                illegal_state,
                "failed to load proven sector {}: {}",
                params.sector_number,
                e
            )),
            Ok(None) => Err(actor_error!(
                not_found,
                "sector {} not proven",
                params.sector_number
            )),
            Ok(Some(_sector)) => Ok(()),
        }
    }

    /// Changes the expiration epoch for a sector to a new, later one.
    /// The sector must not be terminated or faulty.
    /// The sector's power is recomputed for the new expiration.
    /// This method is legacy and should be replaced with calls to extend_sector_expiration2
    fn extend_sector_expiration<BS, RT>(
        rt: &mut RT,
        params: ExtendSectorExpirationParams,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let extend_expiration_inner =
            validate_legacy_extension_declarations(&params.extensions, rt.policy())?;
        Self::extend_sector_expiration_inner(
            rt,
            extend_expiration_inner,
            ExtensionKind::ExtendCommittmentLegacy,
        )
    }

    // Up to date version of extend_sector_expiration that correctly handles simple qap sectors
    // with FIL+ claims. Extension is only allowed if all claim max terms extend past new expiration
    // or claims are dropped.  Power only changes when claims are dropped.
    fn extend_sector_expiration2<BS, RT>(
        rt: &mut RT,
        params: ExtendSectorExpiration2Params,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let extend_expiration_inner = validate_extension_declarations(rt, params.extensions)?;
        Self::extend_sector_expiration_inner(
            rt,
            extend_expiration_inner,
            ExtensionKind::ExtendCommittment,
        )
    }

    fn extend_sector_expiration_inner<BS, RT>(
        rt: &mut RT,
        inner: ExtendExpirationsInner,
        kind: ExtensionKind,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let curr_epoch = rt.curr_epoch();

        /* Loop over sectors and do extension */
        let (power_delta, pledge_delta) = rt.transaction(|state: &mut State, rt| {
            let info = get_miner_info(rt.store(), state)?;
            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            let mut deadlines = state
                .load_deadlines(rt.store())
                .map_err(|e| e.wrap("failed to load deadlines"))?;

            // Group declarations by deadline, and remember iteration order.
            //
            let mut decls_by_deadline: Vec<_> = iter::repeat_with(Vec::new)
                .take(rt.policy().wpost_period_deadlines as usize)
                .collect();
            let mut deadlines_to_load = Vec::<u64>::new();
            for decl in &inner.extensions {
                // the deadline indices are already checked.
                let decls = &mut decls_by_deadline[decl.deadline as usize];
                if decls.is_empty() {
                    deadlines_to_load.push(decl.deadline);
                }
                decls.push(decl);
            }

            let mut sectors = Sectors::load(rt.store(), &state.sectors).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to load sectors array")
            })?;

            let mut power_delta = PowerPair::zero();
            let mut pledge_delta = TokenAmount::zero();

            for deadline_idx in deadlines_to_load {
                let policy = rt.policy();
                let mut deadline = deadlines
                    .load_deadline(policy, rt.store(), deadline_idx)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            format!("failed to load deadline {}", deadline_idx),
                        )
                    })?;

                let mut partitions = deadline.partitions_amt(rt.store()).map_err(|e| {
                    e.downcast_default(
                        ExitCode::USR_ILLEGAL_STATE,
                        format!("failed to load partitions for deadline {}", deadline_idx),
                    )
                })?;

                let quant = state.quant_spec_for_deadline(policy, deadline_idx);

                // Group modified partitions by epoch to which they are extended. Duplicates are ok.
                let mut partitions_by_new_epoch = BTreeMap::<ChainEpoch, Vec<u64>>::new();
                let mut epochs_to_reschedule = Vec::<ChainEpoch>::new();

                for decl in &mut decls_by_deadline[deadline_idx as usize] {
                    let key = PartitionKey {
                        deadline: deadline_idx,
                        partition: decl.partition,
                    };

                    let mut partition = partitions
                        .get(decl.partition)
                        .map_err(|e| {
                            e.downcast_default(
                                ExitCode::USR_ILLEGAL_STATE,
                                format!("failed to load partition {:?}", key),
                            )
                        })?
                        .cloned()
                        .ok_or_else(|| actor_error!(not_found, "no such partition {:?}", key))?;

                    let old_sectors = sectors
                        .load_sector(&decl.sectors)
                        .map_err(|e| e.wrap("failed to load sectors"))?;
                    let new_sectors: Vec<SectorOnChainInfo> = old_sectors
                        .iter()
                        .map(|sector| match kind {
                            ExtensionKind::ExtendCommittmentLegacy => {
                                extend_sector_committment_legacy(
                                    rt.policy(),
                                    curr_epoch,
                                    decl.new_expiration,
                                    sector,
                                )
                            }
                            ExtensionKind::ExtendCommittment => match &inner.claims {
                                None => Err(actor_error!(
                                    unspecified,
                                    "extend2 always specifies (potentially empty) claim mapping"
                                )),
                                Some(claim_space_by_sector) => extend_sector_committment(
                                    rt.policy(),
                                    curr_epoch,
                                    decl.new_expiration,
                                    sector,
                                    claim_space_by_sector,
                                ),
                            },
                        })
                        .collect::<Result<_, _>>()?;

                    // Overwrite sector infos.
                    sectors.store(new_sectors.clone()).map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            format!("failed to update sectors {:?}", decl.sectors),
                        )
                    })?;

                    // Remove old sectors from partition and assign new sectors.
                    let (partition_power_delta, partition_pledge_delta) = partition
                        .replace_sectors(
                            rt.store(),
                            &old_sectors,
                            &new_sectors,
                            info.sector_size,
                            quant,
                        )
                        .map_err(|e| {
                            e.downcast_default(
                                ExitCode::USR_ILLEGAL_STATE,
                                format!("failed to replace sector expirations at {:?}", key),
                            )
                        })?;

                    power_delta += &partition_power_delta;
                    pledge_delta += partition_pledge_delta; // expected to be zero, see note below.

                    partitions.set(decl.partition, partition).map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
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
                        ExitCode::USR_ILLEGAL_STATE,
                        format!("failed to save partitions for deadline {}", deadline_idx),
                    )
                })?;

                // Record partitions in deadline expiration queue
                for epoch in epochs_to_reschedule {
                    let p_idxs = partitions_by_new_epoch.get(&epoch).unwrap();
                    deadline
                        .add_expiration_partitions(rt.store(), epoch, p_idxs, quant)
                        .map_err(|e| {
                            e.downcast_default(
                                ExitCode::USR_ILLEGAL_STATE,
                                format!(
                                    "failed to add expiration partitions to \
                                        deadline {} epoch {}",
                                    deadline_idx, epoch
                                ),
                            )
                        })?;
                }

                deadlines
                    .update_deadline(policy, rt.store(), deadline_idx, &deadline)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            format!("failed to save deadline {}", deadline_idx),
                        )
                    })?;
            }

            state.sectors = sectors.amt.flush().map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to save sectors")
            })?;
            state.save_deadlines(rt.store(), deadlines).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to save deadlines")
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
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        // Note: this cannot terminate pre-committed but un-proven sectors.
        // They must be allowed to expire (and deposit burnt).

        {
            let policy = rt.policy();
            if params.terminations.len() as u64 > policy.declarations_max {
                return Err(actor_error!(
                    illegal_argument,
                    "too many declarations when terminating sectors: {} > {}",
                    params.terminations.len(),
                    policy.declarations_max
                ));
            }
        }

        let mut to_process = DeadlineSectorMap::new();

        for term in params.terminations {
            let deadline = term.deadline;
            let partition = term.partition;

            to_process
                .add(rt.policy(), deadline, partition, term.sectors)
                .map_err(|e| {
                    actor_error!(
                        illegal_argument,
                        "failed to process deadline {}, partition {}: {}",
                        deadline,
                        partition,
                        e
                    )
                })?;
        }

        {
            let policy = rt.policy();
            to_process
                .check(
                    policy.addressed_partitions_max,
                    policy.addressed_sectors_max,
                )
                .map_err(|e| {
                    actor_error!(
                        illegal_argument,
                        "cannot process requested parameters: {}",
                        e
                    )
                })?;
        }

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
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to load sectors")
            })?;

            for (deadline_idx, partition_sectors) in to_process.iter() {
                // If the deadline is the current or next deadline to prove, don't allow terminating sectors.
                // We assume that deadlines are immutable when being proven.
                if !deadline_is_mutable(
                    rt.policy(),
                    state.current_proving_period_start(rt.policy(), curr_epoch),
                    deadline_idx,
                    curr_epoch,
                ) {
                    return Err(actor_error!(
                        illegal_argument,
                        "cannot terminate sectors in immutable deadline {}",
                        deadline_idx
                    ));
                }

                let quant = state.quant_spec_for_deadline(rt.policy(), deadline_idx);
                let mut deadline = deadlines
                    .load_deadline(rt.policy(), store, deadline_idx)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            format!("failed to load deadline {}", deadline_idx),
                        )
                    })?;

                let removed_power = deadline
                    .terminate_sectors(
                        rt.policy(),
                        store,
                        &sectors,
                        curr_epoch,
                        partition_sectors,
                        info.sector_size,
                        quant,
                    )
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            format!("failed to terminate sectors in deadline {}", deadline_idx),
                        )
                    })?;

                state.early_terminations.set(deadline_idx);
                power_delta -= &removed_power;

                deadlines
                    .update_deadline(rt.policy(), store, deadline_idx, &deadline)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            format!("failed to update deadline {}", deadline_idx),
                        )
                    })?;
            }

            state.save_deadlines(store, deadlines).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to save deadlines")
            })?;

            Ok((had_early_terminations, power_delta))
        })?;
        let epoch_reward = request_current_epoch_block_reward(rt)?;
        let pwr_total = request_current_total_power(rt)?;

        // Now, try to process these sectors.
        let more = process_early_terminations(
            rt,
            &epoch_reward.this_epoch_reward_smoothed,
            &pwr_total.quality_adj_power_smoothed,
        )?;

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
            .check_balance_invariants(&rt.current_balance())
            .map_err(balance_invariants_broken)?;

        request_update_power(rt, power_delta)?;
        Ok(TerminateSectorsReturn { done: !more })
    }

    fn declare_faults<BS, RT>(rt: &mut RT, params: DeclareFaultsParams) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        {
            let policy = rt.policy();
            if params.faults.len() as u64 > policy.declarations_max {
                return Err(actor_error!(
                    illegal_argument,
                    "too many fault declarations for a single message: {} > {}",
                    params.faults.len(),
                    policy.declarations_max
                ));
            }
        }

        let mut to_process = DeadlineSectorMap::new();

        for term in params.faults {
            let deadline = term.deadline;
            let partition = term.partition;

            to_process
                .add(rt.policy(), deadline, partition, term.sectors)
                .map_err(|e| {
                    actor_error!(
                        illegal_argument,
                        "failed to process deadline {}, partition {}: {}",
                        deadline,
                        partition,
                        e
                    )
                })?;
        }

        {
            let policy = rt.policy();
            to_process
                .check(
                    policy.addressed_partitions_max,
                    policy.addressed_sectors_max,
                )
                .map_err(|e| {
                    actor_error!(
                        illegal_argument,
                        "cannot process requested parameters: {}",
                        e
                    )
                })?;
        }

        let power_delta = rt.transaction(|state: &mut State, rt| {
            let info = get_miner_info(rt.store(), state)?;

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
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to load sectors array")
            })?;

            let mut new_fault_power_total = PowerPair::zero();
            let curr_epoch = rt.curr_epoch();
            for (deadline_idx, partition_map) in to_process.iter() {
                let policy = rt.policy();
                let target_deadline = declaration_deadline_info(
                    policy,
                    state.current_proving_period_start(policy, curr_epoch),
                    deadline_idx,
                    curr_epoch,
                )
                .map_err(|e| {
                    actor_error!(
                        illegal_argument,
                        "invalid fault declaration deadline {}: {}",
                        deadline_idx,
                        e
                    )
                })?;

                validate_fr_declaration_deadline(&target_deadline).map_err(|e| {
                    actor_error!(
                        illegal_argument,
                        "failed fault declaration at deadline {}: {}",
                        deadline_idx,
                        e
                    )
                })?;

                let mut deadline = deadlines
                    .load_deadline(policy, store, deadline_idx)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            format!("failed to load deadline {}", deadline_idx),
                        )
                    })?;

                let fault_expiration_epoch = target_deadline.last() + policy.fault_max_age;

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
                            ExitCode::USR_ILLEGAL_STATE,
                            format!("failed to declare faults for deadline {}", deadline_idx),
                        )
                    })?;

                deadlines
                    .update_deadline(policy, store, deadline_idx, &deadline)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            format!("failed to store deadline {} partitions", deadline_idx),
                        )
                    })?;

                new_fault_power_total += &deadline_power_delta;
            }

            state.save_deadlines(store, deadlines).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to save deadlines")
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
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        {
            let policy = rt.policy();
            if params.recoveries.len() as u64 > policy.declarations_max {
                return Err(actor_error!(
                    illegal_argument,
                    "too many recovery declarations for a single message: {} > {}",
                    params.recoveries.len(),
                    policy.declarations_max
                ));
            }
        }

        let mut to_process = DeadlineSectorMap::new();

        for term in params.recoveries {
            let deadline = term.deadline;
            let partition = term.partition;

            to_process
                .add(rt.policy(), deadline, partition, term.sectors)
                .map_err(|e| {
                    actor_error!(
                        illegal_argument,
                        "failed to process deadline {}, partition {}: {}",
                        deadline,
                        partition,
                        e
                    )
                })?;
        }

        {
            let policy = rt.policy();
            to_process
                .check(
                    policy.addressed_partitions_max,
                    policy.addressed_sectors_max,
                )
                .map_err(|e| {
                    actor_error!(
                        illegal_argument,
                        "cannot process requested parameters: {}",
                        e
                    )
                })?;
        }

        let fee_to_burn = rt.transaction(|state: &mut State, rt| {
            // Verify unlocked funds cover both InitialPledgeRequirement and FeeDebt
            // and repay fee debt now.
            let fee_to_burn = repay_debts_or_abort(rt, state)?;

            let info = get_miner_info(rt.store(), state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            if consensus_fault_active(&info, rt.curr_epoch()) {
                return Err(actor_error!(
                    forbidden,
                    "recovery not allowed during active consensus fault"
                ));
            }

            let store = rt.store();

            let mut deadlines = state
                .load_deadlines(store)
                .map_err(|e| e.wrap("failed to load deadlines"))?;

            let sectors = Sectors::load(store, &state.sectors).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to load sectors array")
            })?;
            let curr_epoch = rt.curr_epoch();
            for (deadline_idx, partition_map) in to_process.iter() {
                let policy = rt.policy();
                let target_deadline = declaration_deadline_info(
                    policy,
                    state.current_proving_period_start(policy, curr_epoch),
                    deadline_idx,
                    curr_epoch,
                )
                .map_err(|e| {
                    actor_error!(
                        illegal_argument,
                        "invalid recovery declaration deadline {}: {}",
                        deadline_idx,
                        e
                    )
                })?;

                validate_fr_declaration_deadline(&target_deadline).map_err(|e| {
                    actor_error!(
                        illegal_argument,
                        "failed recovery declaration at deadline {}: {}",
                        deadline_idx,
                        e
                    )
                })?;

                let mut deadline = deadlines
                    .load_deadline(policy, store, deadline_idx)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            format!("failed to load deadline {}", deadline_idx),
                        )
                    })?;

                deadline
                    .declare_faults_recovered(store, &sectors, info.sector_size, partition_map)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            format!("failed to declare recoveries for deadline {}", deadline_idx),
                        )
                    })?;

                deadlines
                    .update_deadline(policy, store, deadline_idx, &deadline)
                    .map_err(|e| {
                        e.downcast_default(
                            ExitCode::USR_ILLEGAL_STATE,
                            format!("failed to store deadline {}", deadline_idx),
                        )
                    })?;
            }

            state.save_deadlines(store, deadlines).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to save deadlines")
            })?;

            Ok(fee_to_burn)
        })?;

        burn_funds(rt, fee_to_burn)?;
        let state: State = rt.state()?;
        state
            .check_balance_invariants(&rt.current_balance())
            .map_err(balance_invariants_broken)?;

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
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        {
            let policy = rt.policy();
            if params.deadline >= policy.wpost_period_deadlines {
                return Err(actor_error!(
                    illegal_argument,
                    "invalid deadline {}",
                    params.deadline
                ));
            }
        }

        let partitions = params.partitions.validate().map_err(|e| {
            actor_error!(
                illegal_argument,
                "failed to parse partitions bitfield: {}",
                e
            )
        })?;
        let partition_count = partitions.len();

        let params_deadline = params.deadline;

        rt.transaction(|state: &mut State, rt| {
            let info = get_miner_info(rt.store(), state)?;

            rt.validate_immediate_caller_is(
                info.control_addresses
                    .iter()
                    .chain(&[info.worker, info.owner]),
            )?;

            let store = rt.store();
            let policy = rt.policy();

            if !deadline_available_for_compaction(
                policy,
                state.current_proving_period_start(policy, rt.curr_epoch()),
                params_deadline,
                rt.curr_epoch(),
            ) {
                return Err(actor_error!(
                    forbidden,
                    "cannot compact deadline {} during its challenge window, \
                    or the prior challenge window,
                    or before {} epochs have passed since its last challenge window ended",
                    params_deadline,
                    policy.wpost_dispute_window
                ));
            }

            let submission_partition_limit =
                load_partitions_sectors_max(policy, info.window_post_partition_sectors);
            if partition_count > submission_partition_limit {
                return Err(actor_error!(
                    illegal_argument,
                    "too many partitions {}, limit {}",
                    partition_count,
                    submission_partition_limit
                ));
            }

            let quant = state.quant_spec_for_deadline(policy, params_deadline);
            let mut deadlines = state
                .load_deadlines(store)
                .map_err(|e| e.wrap("failed to load deadlines"))?;

            let mut deadline = deadlines
                .load_deadline(policy, store, params_deadline)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::USR_ILLEGAL_STATE,
                        format!("failed to load deadline {}", params_deadline),
                    )
                })?;

            let (live, dead, removed_power) = deadline
                .remove_partitions(store, partitions, quant)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::USR_ILLEGAL_STATE,
                        format!(
                            "failed to remove partitions from deadline {}",
                            params_deadline
                        ),
                    )
                })?;

            state.delete_sectors(store, &dead).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to delete dead sectors")
            })?;

            let sectors = state.load_sector_infos(store, &live).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to load moved sectors")
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
                        ExitCode::USR_ILLEGAL_STATE,
                        "failed to add back moved sectors",
                    )
                })?;

            if removed_power != added_power {
                return Err(actor_error!(
                    illegal_state,
                    "power changed when compacting partitions: was {:?}, is now {:?}",
                    removed_power,
                    added_power
                ));
            }

            deadlines
                .update_deadline(policy, store, params_deadline, &deadline)
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::USR_ILLEGAL_STATE,
                        format!("failed to update deadline {}", params_deadline),
                    )
                })?;

            state.save_deadlines(store, deadlines).map_err(|e| {
                e.downcast_default(
                    ExitCode::USR_ILLEGAL_STATE,
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
        params: CompactSectorNumbersParams,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        let mask_sector_numbers = params
            .mask_sector_numbers
            .validate()
            .map_err(|e| actor_error!(illegal_argument, "invalid mask bitfield: {}", e))?;

        let last_sector_number = mask_sector_numbers
            .last()
            .ok_or_else(|| actor_error!(illegal_argument, "invalid mask bitfield"))?
            as SectorNumber;

        if last_sector_number > MAX_SECTOR_NUMBER {
            return Err(actor_error!(
                illegal_argument,
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

            state.allocate_sector_numbers(
                rt.store(),
                mask_sector_numbers,
                CollisionPolicy::AllowCollisions,
            )
        })?;

        Ok(())
    }

    /// Locks up some amount of a the miner's unlocked balance (including funds received alongside the invoking message).
    fn apply_rewards<BS, RT>(rt: &mut RT, params: ApplyRewardParams) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        if params.reward.is_negative() {
            return Err(actor_error!(
                illegal_argument,
                "cannot lock up a negative amount of funds"
            ));
        }
        if params.penalty.is_negative() {
            return Err(actor_error!(
                illegal_argument,
                "cannot penalize a negative amount of funds"
            ));
        }

        let (pledge_delta_total, to_burn) = rt.transaction(|st: &mut State, rt| {
            let mut pledge_delta_total = TokenAmount::zero();

            rt.validate_immediate_caller_is(std::iter::once(&REWARD_ACTOR_ADDR))?;

            let (reward_to_lock, locked_reward_vesting_spec) =
                locked_reward_from_reward(params.reward);

            // This ensures the miner has sufficient funds to lock up amountToLock.
            // This should always be true if reward actor sends reward funds with the message.
            let unlocked_balance = st
                .get_unlocked_balance(&rt.current_balance())
                .map_err(|e| {
                    actor_error!(illegal_state, "failed to calculate unlocked balance: {}", e)
                })?;

            if unlocked_balance < reward_to_lock {
                return Err(actor_error!(
                    insufficient_funds,
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
                        illegal_state,
                        "failed to lock funds in vesting table: {}",
                        e
                    )
                })?;
            pledge_delta_total -= &newly_vested;
            pledge_delta_total += &reward_to_lock;

            st.apply_penalty(&params.penalty)
                .map_err(|e| actor_error!(illegal_state, "failed to apply penalty: {}", e))?;

            // Attempt to repay all fee debt in this call. In most cases the miner will have enough
            // funds in the *reward alone* to cover the penalty. In the rare case a miner incurs more
            // penalty than it can pay for with reward and existing funds, it will go into fee debt.
            let (penalty_from_vesting, penalty_from_balance) = st
                .repay_partial_debt_in_priority_order(
                    rt.store(),
                    rt.curr_epoch(),
                    &rt.current_balance(),
                )
                .map_err(|e| {
                    e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to repay penalty")
                })?;
            pledge_delta_total -= &penalty_from_vesting;
            let to_burn = penalty_from_vesting + penalty_from_balance;
            Ok((pledge_delta_total, to_burn))
        })?;

        notify_pledge_changed(rt, &pledge_delta_total)?;
        burn_funds(rt, to_burn)?;
        let st: State = rt.state()?;
        st.check_balance_invariants(&rt.current_balance())
            .map_err(balance_invariants_broken)?;
        Ok(())
    }

    fn report_consensus_fault<BS, RT>(
        rt: &mut RT,
        params: ReportConsensusFaultParams,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        // Note: only the first report of any fault is processed because it sets the
        // ConsensusFaultElapsed state variable to an epoch after the fault, and reports prior to
        // that epoch are no longer valid
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;
        let reporter = rt.message().caller();

        let fault = rt
            .verify_consensus_fault(&params.header1, &params.header2, &params.header_extra)
            .map_err(|e| e.downcast_default(ExitCode::USR_ILLEGAL_ARGUMENT, "fault not verified"))?
            .ok_or_else(|| actor_error!(illegal_argument, "No consensus fault found"))?;
        if fault.target != rt.message().receiver() {
            return Err(actor_error!(
                illegal_argument,
                "fault by {} reported to miner {}",
                fault.target,
                rt.message().receiver()
            ));
        }

        // Elapsed since the fault (i.e. since the higher of the two blocks)
        let fault_age = rt.curr_epoch() - fault.epoch;
        if fault_age <= 0 {
            return Err(actor_error!(
                illegal_argument,
                "invalid fault epoch {} ahead of current {}",
                fault.epoch,
                rt.curr_epoch()
            ));
        }

        // Reward reporter with a share of the miner's current balance.
        let reward_stats = request_current_epoch_block_reward(rt)?;

        // The policy amounts we should burn and send to reporter
        // These may differ from actual funds send when miner goes into fee debt
        let this_epoch_reward =
            TokenAmount::from_atto(reward_stats.this_epoch_reward_smoothed.estimate());
        let fault_penalty = consensus_fault_penalty(this_epoch_reward.clone());
        let slasher_reward = reward_for_consensus_slash_report(&this_epoch_reward);

        let mut pledge_delta = TokenAmount::zero();

        let (burn_amount, reward_amount) = rt.transaction(|st: &mut State, rt| {
            let mut info = get_miner_info(rt.store(), st)?;

            // Verify miner hasn't already been faulted
            if fault.epoch < info.consensus_fault_elapsed {
                return Err(actor_error!(
                    forbidden,
                    "fault epoch {} is too old, last exclusion period ended at {}",
                    fault.epoch,
                    info.consensus_fault_elapsed
                ));
            }

            st.apply_penalty(&fault_penalty).map_err(|e| {
                actor_error!(illegal_state, format!("failed to apply penalty: {}", e))
            })?;

            // Pay penalty
            let (penalty_from_vesting, penalty_from_balance) = st
                .repay_partial_debt_in_priority_order(
                    rt.store(),
                    rt.curr_epoch(),
                    &rt.current_balance(),
                )
                .map_err(|e| {
                    e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to pay fees")
                })?;

            let mut burn_amount = &penalty_from_vesting + &penalty_from_balance;
            pledge_delta -= penalty_from_vesting;

            // clamp reward at funds burnt
            let reward_amount = std::cmp::min(&burn_amount, &slasher_reward).clone();
            burn_amount -= &reward_amount;

            info.consensus_fault_elapsed =
                rt.curr_epoch() + rt.policy().consensus_fault_ineligibility_duration;

            st.save_info(rt.store(), &info).map_err(|e| {
                e.downcast_default(ExitCode::USR_SERIALIZATION, "failed to save miner info")
            })?;

            Ok((burn_amount, reward_amount))
        })?;

        if let Err(e) = rt.send(&reporter, METHOD_SEND, RawBytes::default(), reward_amount) {
            error!("failed to send reward: {}", e);
        }

        burn_funds(rt, burn_amount)?;
        notify_pledge_changed(rt, &pledge_delta)?;

        let state: State = rt.state()?;
        state
            .check_balance_invariants(&rt.current_balance())
            .map_err(balance_invariants_broken)?;
        Ok(())
    }

    fn withdraw_balance<BS, RT>(
        rt: &mut RT,
        params: WithdrawBalanceParams,
    ) -> Result<WithdrawBalanceReturn, ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        if params.amount_requested.is_negative() {
            return Err(actor_error!(
                illegal_argument,
                "negative fund requested for withdrawal: {}",
                params.amount_requested
            ));
        }

        let (info, amount_withdrawn, newly_vested, fee_to_burn, state) =
            rt.transaction(|state: &mut State, rt| {
                let mut info = get_miner_info(rt.store(), state)?;

                // Only the owner is allowed to withdraw the balance as it belongs to/is controlled by the owner
                // and not the worker.
                rt.validate_immediate_caller_is(&[info.owner, info.beneficiary])?;

                // Ensure we don't have any pending terminations.
                if !state.early_terminations.is_empty() {
                    return Err(actor_error!(
                        forbidden,
                        "cannot withdraw funds while {} deadlines have terminated sectors \
                        with outstanding fees",
                        state.early_terminations.len()
                    ));
                }

                // Unlock vested funds so we can spend them.
                let newly_vested =
                    state.unlock_vested_funds(rt.store(), rt.curr_epoch()).map_err(|e| {
                        e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "Failed to vest fund")
                    })?;

                // available balance already accounts for fee debt so it is correct to call
                // this before RepayDebts. We would have to
                // subtract fee debt explicitly if we called this after.
                let available_balance =
                    state.get_available_balance(&rt.current_balance()).map_err(|e| {
                        actor_error!(
                            illegal_state,
                            format!("failed to calculate available balance: {}", e)
                        )
                    })?;

                // Verify unlocked funds cover both InitialPledgeRequirement and FeeDebt
                // and repay fee debt now.
                let fee_to_burn = repay_debts_or_abort(rt, state)?;
                let mut amount_withdrawn =
                    std::cmp::min(&available_balance, &params.amount_requested);
                if amount_withdrawn.is_negative() {
                    return Err(actor_error!(
                        illegal_state,
                        "negative amount to withdraw: {}",
                        amount_withdrawn
                    ));
                }
                if info.beneficiary != info.owner {
                    // remaining_quota always zero and positive
                    let remaining_quota = info.beneficiary_term.available(rt.curr_epoch());
                    if remaining_quota.is_zero() {
                        return Err(actor_error!(
                            forbidden,
                            "beneficiary expiration of epoch {} passed or quota of {} depleted with {} used",
                            info.beneficiary_term.expiration,
                            info.beneficiary_term.quota,
                            info.beneficiary_term.used_quota
                        ));
                    }
                    amount_withdrawn = std::cmp::min(amount_withdrawn, &remaining_quota);
                    if amount_withdrawn.is_positive() {
                        info.beneficiary_term.used_quota += amount_withdrawn;
                        state.save_info(rt.store(), &info).map_err(|e| {
                            e.downcast_default(
                                ExitCode::USR_ILLEGAL_STATE,
                                "failed to save miner info",
                            )
                        })?;
                    }
                    Ok((info, amount_withdrawn.clone(), newly_vested, fee_to_burn, state.clone()))
                } else {
                    Ok((info, amount_withdrawn.clone(), newly_vested, fee_to_burn, state.clone()))
                }
            })?;

        if amount_withdrawn.is_positive() {
            rt.send(
                &info.beneficiary,
                METHOD_SEND,
                RawBytes::default(),
                amount_withdrawn.clone(),
            )?;
        }

        burn_funds(rt, fee_to_burn)?;
        notify_pledge_changed(rt, &newly_vested.neg())?;

        state
            .check_balance_invariants(&rt.current_balance())
            .map_err(balance_invariants_broken)?;
        Ok(WithdrawBalanceReturn { amount_withdrawn })
    }

    /// Proposes or confirms a change of beneficiary address.
    /// A proposal must be submitted by the owner, and takes effect after approval of both the proposed beneficiary and current beneficiary,
    /// if applicable, any current beneficiary that has time and quota remaining.
    //// See FIP-0029, https://github.com/filecoin-project/FIPs/blob/master/FIPS/fip-0029.md
    fn change_beneficiary<BS, RT>(
        rt: &mut RT,
        params: ChangeBeneficiaryParams,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        let caller = rt.message().caller();
        let new_beneficiary =
            Address::new_id(rt.resolve_address(&params.new_beneficiary).ok_or_else(|| {
                actor_error!(
                    illegal_argument,
                    "unable to resolve address: {}",
                    params.new_beneficiary
                )
            })?);

        rt.transaction(|state: &mut State, rt| {
            let mut info = get_miner_info(rt.store(), state)?;
            if caller == info.owner {
                // This is a ChangeBeneficiary proposal when the caller is Owner
                if new_beneficiary != info.owner {
                    // When beneficiary is not owner, just check quota in params,
                    // Expiration maybe an expiration value, but wouldn't cause problem, just the new beneficiary never get any benefit
                    if !params.new_quota.is_positive() {
                        return Err(actor_error!(
                            illegal_argument,
                            "beneficial quota {} must bigger than zero",
                            params.new_quota
                        ));
                    }
                } else {
                    // Expiration/quota must set to 0 while change beneficiary to owner
                    if !params.new_quota.is_zero() {
                        return Err(actor_error!(
                            illegal_argument,
                            "owner beneficial quota {} must be zero",
                            params.new_quota
                        ));
                    }

                    if params.new_expiration != 0 {
                        return Err(actor_error!(
                            illegal_argument,
                            "owner beneficial expiration {} must be zero",
                            params.new_expiration
                        ));
                    }
                }

                let mut pending_beneficiary_term = PendingBeneficiaryChange::new(
                    new_beneficiary,
                    params.new_quota,
                    params.new_expiration,
                );
                if info.beneficiary_term.available(rt.curr_epoch()).is_zero() {
                    // Set current beneficiary to approved when current beneficiary is not effective
                    pending_beneficiary_term.approved_by_beneficiary = true;
                }
                info.pending_beneficiary_term = Some(pending_beneficiary_term);
            } else if let Some(pending_term) = &info.pending_beneficiary_term {
                if caller != info.beneficiary && caller != pending_term.new_beneficiary {
                    return Err(actor_error!(
                        forbidden,
                        "message caller {} is neither proposal beneficiary{} nor current beneficiary{}",
                        caller,
                        params.new_beneficiary,
                        info.beneficiary
                    ));
                }

                if pending_term.new_beneficiary != new_beneficiary {
                    return Err(actor_error!(
                        illegal_argument,
                        "new beneficiary address must be equal expect {}, but got {}",
                        pending_term.new_beneficiary,
                        params.new_beneficiary
                    ));
                }
                if pending_term.new_quota != params.new_quota {
                    return Err(actor_error!(
                        illegal_argument,
                        "new beneficiary quota must be equal expect {}, but got {}",
                        pending_term.new_quota,
                        params.new_quota
                    ));
                }
                if pending_term.new_expiration != params.new_expiration {
                    return Err(actor_error!(
                        illegal_argument,
                        "new beneficiary expire date must be equal expect {}, but got {}",
                        pending_term.new_expiration,
                        params.new_expiration
                    ));
                }
            } else {
                return Err(actor_error!(forbidden, "No changeBeneficiary proposal exists"));
            }

            if let Some(pending_term) = info.pending_beneficiary_term.as_mut() {
                if caller == info.beneficiary {
                    pending_term.approved_by_beneficiary = true
                }

                if caller == new_beneficiary {
                    pending_term.approved_by_nominee = true
                }

                if pending_term.approved_by_beneficiary && pending_term.approved_by_nominee {
                    //approved by both beneficiary and nominee
                    if new_beneficiary != info.beneficiary {
                        //if beneficiary changes, reset used_quota to zero
                        info.beneficiary_term.used_quota = TokenAmount::zero();
                    }
                    info.beneficiary = new_beneficiary;
                    info.beneficiary_term.quota = pending_term.new_quota.clone();
                    info.beneficiary_term.expiration = pending_term.new_expiration;
                    // clear the pending proposal
                    info.pending_beneficiary_term = None;
                }
            }

            state.save_info(rt.store(), &info).map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to save miner info")
            })?;
            Ok(())
        })
    }

    // GetBeneficiary retrieves the currently active and proposed beneficiary information.
    // This method is for use by other actors (such as those acting as beneficiaries),
    // and to abstract the state representation for clients.
    fn get_beneficiary<BS, RT>(rt: &mut RT) -> Result<GetBeneficiaryReturn, ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        let info = rt.transaction(|state: &mut State, rt| get_miner_info(rt.store(), state))?;

        Ok(GetBeneficiaryReturn {
            active: ActiveBeneficiary {
                beneficiary: info.beneficiary,
                term: info.beneficiary_term,
            },
            proposed: info.pending_beneficiary_term,
        })
    }

    fn repay_debt<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: Blockstore,
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
                    &rt.current_balance(),
                )
                .map_err(|e| {
                    e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to unlock fee debt")
                })?;

            Ok((from_vesting, from_balance, state.clone()))
        })?;

        let burn_amount = from_balance + &from_vesting;
        notify_pledge_changed(rt, &from_vesting.neg())?;
        burn_funds(rt, burn_amount)?;

        state
            .check_balance_invariants(&rt.current_balance())
            .map_err(balance_invariants_broken)?;
        Ok(())
    }

    fn on_deferred_cron_event<BS, RT>(
        rt: &mut RT,
        params: DeferredCronEventParams,
    ) -> Result<(), ActorError>
    where
        BS: Blockstore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&STORAGE_POWER_ACTOR_ADDR))?;

        let payload: CronEventPayload = from_slice(&params.event_payload).map_err(|e| {
            actor_error!(
                illegal_state,
                format!(
                    "failed to unmarshal miner cron payload into expected structure: {}",
                    e
                )
            )
        })?;

        match payload.event_type {
            CRON_EVENT_PROVING_DEADLINE => handle_proving_deadline(
                rt,
                &params.reward_smoothed,
                &params.quality_adj_power_smoothed,
            )?,
            CRON_EVENT_PROCESS_EARLY_TERMINATIONS => {
                if process_early_terminations(
                    rt,
                    &params.reward_smoothed,
                    &params.quality_adj_power_smoothed,
                )? {
                    schedule_early_termination_work(rt)?
                }
            }
            _ => {
                error!(
                    "onDeferredCronEvent invalid event type: {}",
                    payload.event_type
                );
            }
        };
        let state: State = rt.state()?;
        state
            .check_balance_invariants(&rt.current_balance())
            .map_err(balance_invariants_broken)?;
        Ok(())
    }
}

#[derive(Debug, PartialEq, Clone)]
struct SectorPreCommitInfoInner {
    pub seal_proof: RegisteredSealProof,
    pub sector_number: SectorNumber,
    /// CommR
    pub sealed_cid: Cid,
    pub seal_rand_epoch: ChainEpoch,
    pub deal_ids: Vec<DealID>,
    pub expiration: ChainEpoch,
    /// CommD
    pub unsealed_cid: Option<CompactCommD>,
}

/// ReplicaUpdate param with Option<Cid> for CommD
/// None means unknown
pub struct ReplicaUpdateInner {
    pub sector_number: SectorNumber,
    pub deadline: u64,
    pub partition: u64,
    pub new_sealed_cid: Cid,
    /// None means unknown
    pub new_unsealed_cid: Option<Cid>,
    pub deals: Vec<DealID>,
    pub update_proof_type: RegisteredUpdateProof,
    pub replica_proof: Vec<u8>,
}

enum ExtensionKind {
    ExtendCommittmentLegacy, // handle only legacy sectors
    ExtendCommittment,       // handle both Simple QAP and legacy sectors
                             // TODO: when landing https://github.com/filecoin-project/builtin-actors/pull/518
                             // ExtendProofValidity
}

// ExtendSectorExpiration param
struct ExtendExpirationsInner {
    extensions: Vec<ValidatedExpirationExtension>,
    // Map from sector being extended to (check, maintain)
    // `check` is the space of active claims, checked to ensure all claims are checked
    // `maintain` is the space of claims to maintain
    // maintain <= check with equality in the case no claims are dropped
    claims: Option<BTreeMap<SectorNumber, (u64, u64)>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedExpirationExtension {
    pub deadline: u64,
    pub partition: u64,
    pub sectors: BitField,
    pub new_expiration: ChainEpoch,
}

#[allow(clippy::too_many_arguments)] // validate mut prevents implementing From
impl From<ExpirationExtension2> for ValidatedExpirationExtension {
    fn from(e2: ExpirationExtension2) -> Self {
        let mut sectors = BitField::new();
        for sc in e2.sectors_with_claims {
            sectors.set(sc.sector_number)
        }
        sectors |= &e2.sectors;

        Self {
            deadline: e2.deadline,
            partition: e2.partition,
            sectors,
            new_expiration: e2.new_expiration,
        }
    }
}

fn validate_legacy_extension_declarations(
    extensions: &[ExpirationExtension],
    policy: &Policy,
) -> Result<ExtendExpirationsInner, ActorError> {
    let vec_validated = extensions
        .iter()
        .map(|decl| {
            if decl.deadline >= policy.wpost_period_deadlines {
                return Err(actor_error!(
                    illegal_argument,
                    "deadline {} not in range 0..{}",
                    decl.deadline,
                    policy.wpost_period_deadlines
                ));
            }

            Ok(ValidatedExpirationExtension {
                deadline: decl.deadline,
                partition: decl.partition,
                sectors: decl.sectors.clone(),
                new_expiration: decl.new_expiration,
            })
        })
        .collect::<Result<_, _>>()?;

    Ok(ExtendExpirationsInner {
        extensions: vec_validated,
        claims: None,
    })
}

fn validate_extension_declarations<BS, RT>(
    rt: &mut RT,
    extensions: Vec<ExpirationExtension2>,
) -> Result<ExtendExpirationsInner, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let mut claim_space_by_sector = BTreeMap::<SectorNumber, (u64, u64)>::new();

    for decl in &extensions {
        let policy = rt.policy();
        if decl.deadline >= policy.wpost_period_deadlines {
            return Err(actor_error!(
                illegal_argument,
                "deadline {} not in range 0..{}",
                decl.deadline,
                policy.wpost_period_deadlines
            ));
        }

        for sc in &decl.sectors_with_claims {
            let mut drop_claims = sc.drop_claims.clone();
            let mut all_claim_ids = sc.maintain_claims.clone();
            all_claim_ids.append(&mut drop_claims);
            let claims = get_claims(rt, &all_claim_ids)
                .with_context(|| format!("failed to get claims for sector {}", sc.sector_number))?;
            let first_drop = sc.maintain_claims.len();

            for (i, claim) in claims.iter().enumerate() {
                // check provider and sector matches
                if claim.provider != rt.message().receiver().id().unwrap() {
                    return Err(actor_error!(illegal_argument, "failed to validate declaration sector={}, claim={}, expected claim provider to be {} but found {} ", sc.sector_number, all_claim_ids[i], rt.message().receiver().id().unwrap(), claim.provider));
                }
                if claim.sector != sc.sector_number {
                    return Err(actor_error!(illegal_argument, "failed to validate declaration sector={}, claim={} expected claim sector number to be {} but found {} ", sc.sector_number, all_claim_ids[i], sc.sector_number, claim.sector));
                }

                // If we are not dropping check expiration does not exceed term max
                let mut maintain_delta: u64 = 0;
                if i < first_drop {
                    if decl.new_expiration > claim.term_start + claim.term_max {
                        return Err(actor_error!(forbidden, "failed to validate declaration sector={}, claim={} claim only allows extension to {} but declared new expiration is {}", sc.sector_number, sc.maintain_claims[i], claim.term_start + claim.term_max, decl.new_expiration));
                    }
                    maintain_delta = claim.size.0
                }

                claim_space_by_sector
                    .entry(sc.sector_number)
                    .and_modify(|(check, maintain)| {
                        *check += claim.size.0;
                        *maintain += maintain_delta;
                    })
                    .or_insert((claim.size.0, maintain_delta));
            }
        }
    }
    Ok(ExtendExpirationsInner {
        extensions: extensions.into_iter().map(|e2| e2.into()).collect(),
        claims: Some(claim_space_by_sector),
    })
}

fn extend_sector_committment(
    policy: &Policy,
    curr_epoch: ChainEpoch,
    new_expiration: ChainEpoch,
    sector: &SectorOnChainInfo,
    claim_space_by_sector: &BTreeMap<SectorNumber, (u64, u64)>,
) -> Result<SectorOnChainInfo, ActorError> {
    validate_extended_expiration(policy, curr_epoch, new_expiration, sector)?;

    // all simple_qa_power sectors with VerifiedDealWeight > 0 MUST check all claims
    if sector.simple_qa_power {
        extend_simple_qap_sector(
            policy,
            new_expiration,
            curr_epoch,
            sector,
            claim_space_by_sector,
        )
    } else {
        extend_non_simple_qap_sector(new_expiration, curr_epoch, sector)
    }
}

fn extend_sector_committment_legacy(
    policy: &Policy,
    curr_epoch: ChainEpoch,
    new_expiration: ChainEpoch,
    sector: &SectorOnChainInfo,
) -> Result<SectorOnChainInfo, ActorError> {
    validate_extended_expiration(policy, curr_epoch, new_expiration, sector)?;

    // it is an error to do legacy sector expiration on simple-qa power sectors with deal weight
    if sector.simple_qa_power
        && (sector.verified_deal_weight > BigInt::zero() || sector.deal_weight > BigInt::zero())
    {
        return Err(actor_error!(
            forbidden,
            "cannot use legacy sector extension for simple qa power with deal weight {}",
            sector.sector_number
        ));
    }
    extend_non_simple_qap_sector(new_expiration, curr_epoch, sector)
}

fn validate_extended_expiration(
    policy: &Policy,
    curr_epoch: ChainEpoch,
    new_expiration: ChainEpoch,
    sector: &SectorOnChainInfo,
) -> Result<(), ActorError> {
    if !can_extend_seal_proof_type(sector.seal_proof) {
        return Err(actor_error!(
            forbidden,
            "cannot extend expiration for sector {} with unsupported \
            seal type {:?}",
            sector.sector_number,
            sector.seal_proof
        ));
    }
    // This can happen if the sector should have already expired, but hasn't
    // because the end of its deadline hasn't passed yet.
    if sector.expiration < curr_epoch {
        return Err(actor_error!(
            forbidden,
            "cannot extend expiration for expired sector {} at {}",
            sector.sector_number,
            sector.expiration
        ));
    }

    if new_expiration < sector.expiration {
        return Err(actor_error!(
            illegal_argument,
            "cannot reduce sector {} expiration to {} from {}",
            sector.sector_number,
            new_expiration,
            sector.expiration
        ));
    }

    validate_expiration(
        policy,
        curr_epoch,
        sector.activation,
        new_expiration,
        sector.seal_proof,
    )?;
    Ok(())
}

fn extend_simple_qap_sector(
    policy: &Policy,
    new_expiration: ChainEpoch,
    curr_epoch: ChainEpoch,
    sector: &SectorOnChainInfo,
    claim_space_by_sector: &BTreeMap<SectorNumber, (u64, u64)>,
) -> Result<SectorOnChainInfo, ActorError> {
    let mut new_sector = sector.clone();
    if sector.verified_deal_weight > BigInt::zero() {
        let old_duration = sector.expiration - sector.activation;
        let deal_space = &sector.deal_weight / old_duration;
        let old_verified_deal_space = &sector.verified_deal_weight / old_duration;
        let (expected_verified_deal_space, new_verified_deal_space) =
            match claim_space_by_sector.get(&sector.sector_number) {
                None => {
                    return Err(actor_error!(
                        illegal_argument,
                        "claim missing from declaration for sector {} with non-zero verified deal weight {}",
                        sector.sector_number,
                        &sector.verified_deal_weight
                    ))
                }
                Some(space) => space,
            };
        // claims must be completely accounted for
        if BigInt::from(*expected_verified_deal_space as i64) != old_verified_deal_space {
            return Err(actor_error!(illegal_argument, "declared verified deal space in claims ({}) does not match verified deal space ({}) for sector {}", expected_verified_deal_space, old_verified_deal_space, sector.sector_number));
        }
        // claim dropping is restricted to extensions at the end of a sector's life

        let dropping_claims = expected_verified_deal_space != new_verified_deal_space;
        if dropping_claims && sector.expiration - curr_epoch > policy.end_of_life_claim_drop_period
        {
            return Err(actor_error!(
                forbidden,
                "attempt to drop claims with {} epochs > end of life claim drop period {} remaining",
                sector.expiration - curr_epoch,
                policy.end_of_life_claim_drop_period
            ));
        }

        new_sector.expiration = new_expiration;
        // update deal weights to account for new duration
        new_sector.deal_weight = deal_space * (new_sector.expiration - new_sector.activation);
        new_sector.verified_deal_weight = BigInt::from(*new_verified_deal_space)
            * (new_sector.expiration - new_sector.activation);
    } else {
        new_sector.expiration = new_expiration
    }
    Ok(new_sector)
}

fn extend_non_simple_qap_sector(
    new_expiration: ChainEpoch,
    curr_epoch: ChainEpoch,
    sector: &SectorOnChainInfo,
) -> Result<SectorOnChainInfo, ActorError> {
    let mut new_sector = sector.clone();
    // Remove "spent" deal weights for non simple_qa_power sectors with deal weight > 0
    let new_deal_weight = (&sector.deal_weight * (sector.expiration - curr_epoch))
        .div_floor(&BigInt::from(sector.expiration - sector.activation));

    let new_verified_deal_weight = (&sector.verified_deal_weight
        * (sector.expiration - curr_epoch))
        .div_floor(&BigInt::from(sector.expiration - sector.activation));

    new_sector.expiration = new_expiration;
    new_sector.deal_weight = new_deal_weight;
    new_sector.verified_deal_weight = new_verified_deal_weight;
    Ok(new_sector)
}

// TODO: We're using the current power+epoch reward. Technically, we
// should use the power/reward at the time of termination.
// https://github.com/filecoin-project/specs-actors/v6/pull/648
fn process_early_terminations<BS, RT>(
    rt: &mut RT,
    reward_smoothed: &FilterEstimate,
    quality_adj_power_smoothed: &FilterEstimate,
) -> Result</* more */ bool, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let (result, more, deals_to_terminate, penalty, pledge_delta) =
        rt.transaction(|state: &mut State, rt| {
            let store = rt.store();
            let policy = rt.policy();

            let (result, more) = state
                .pop_early_terminations(
                    policy,
                    store,
                    policy.addressed_partitions_max,
                    policy.addressed_sectors_max,
                )
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::USR_ILLEGAL_STATE,
                        "failed to pop early terminations",
                    )
                })?;

            // Nothing to do, don't waste any time.
            // This can happen if we end up processing early terminations
            // before the cron callback fires.
            if result.is_empty() {
                info!("no early terminations (maybe cron callback hasn't happened yet?)");
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
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to load sectors array")
            })?;

            let mut total_initial_pledge = TokenAmount::zero();
            let mut deals_to_terminate =
                Vec::<ext::market::OnMinerSectorsTerminateParams>::with_capacity(
                    result.sectors.len(),
                );
            let mut penalty = TokenAmount::zero();

            for (epoch, sector_numbers) in result.iter() {
                let sectors = sectors
                    .load_sector(sector_numbers)
                    .map_err(|e| e.wrap("failed to load sector infos"))?;

                penalty += termination_penalty(
                    info.sector_size,
                    epoch,
                    reward_smoothed,
                    quality_adj_power_smoothed,
                    &sectors,
                );

                // estimate ~one deal per sector.
                let mut deal_ids = Vec::<DealID>::with_capacity(sectors.len());
                for sector in sectors {
                    deal_ids.extend(sector.deal_ids);
                    total_initial_pledge += sector.initial_pledge;
                }

                let params = ext::market::OnMinerSectorsTerminateParams { epoch, deal_ids };
                deals_to_terminate.push(params);
            }

            // Pay penalty
            state
                .apply_penalty(&penalty)
                .map_err(|e| actor_error!(illegal_state, "failed to apply penalty: {}", e))?;

            // Remove pledge requirement.
            let mut pledge_delta = -total_initial_pledge;
            state.add_initial_pledge(&pledge_delta).map_err(|e| {
                actor_error!(
                    illegal_state,
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
                    &rt.current_balance(),
                )
                .map_err(|e| {
                    e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to repay penalty")
                })?;

            penalty = &penalty_from_vesting + penalty_from_balance;
            pledge_delta -= penalty_from_vesting;

            Ok((result, more, deals_to_terminate, penalty, pledge_delta))
        })?;

    // We didn't do anything, abort.
    if result.is_empty() {
        info!("no early terminations");
        return Ok(more);
    }

    // Burn penalty.
    log::debug!(
        "storage provider {} penalized {} for sector termination",
        rt.message().receiver(),
        penalty
    );
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
fn handle_proving_deadline<BS, RT>(
    rt: &mut RT,
    reward_smoothed: &FilterEstimate,
    quality_adj_power_smoothed: &FilterEstimate,
) -> Result<(), ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let curr_epoch = rt.curr_epoch();

    let mut had_early_terminations = false;

    let mut power_delta_total = PowerPair::zero();
    let mut penalty_total = TokenAmount::zero();
    let mut pledge_delta_total = TokenAmount::zero();
    let mut continue_cron = false;

    let state: State = rt.transaction(|state: &mut State, rt| {
        let policy = rt.policy();
        // Vest locked funds.
        // This happens first so that any subsequent penalties are taken
        // from locked vesting funds before funds free this epoch.
        let newly_vested = state
            .unlock_vested_funds(rt.store(), rt.curr_epoch())
            .map_err(|e| e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to vest funds"))?;

        pledge_delta_total -= newly_vested;

        // Process pending worker change if any
        let mut info = get_miner_info(rt.store(), state)?;
        process_pending_worker(&mut info, rt, state)?;

        let deposit_to_burn = state
            .cleanup_expired_pre_commits(policy, rt.store(), rt.curr_epoch())
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::USR_ILLEGAL_STATE,
                    "failed to expire pre-committed sectors",
                )
            })?;
        state
            .apply_penalty(&deposit_to_burn)
            .map_err(|e| actor_error!(illegal_state, "failed to apply penalty: {}", e))?;

        log::debug!(
            "storage provider {} penalized {} for expired pre commits",
            rt.message().receiver(),
            deposit_to_burn
        );

        // Record whether or not we _had_ early terminations in the queue before this method.
        // That way, don't re-schedule a cron callback if one is already scheduled.
        had_early_terminations = have_pending_early_terminations(state);

        let result = state
            .advance_deadline(policy, rt.store(), rt.curr_epoch())
            .map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to advance deadline")
            })?;

        // Faults detected by this missed PoSt pay no penalty, but sectors that were already faulty
        // and remain faulty through this deadline pay the fault fee.
        let penalty_target = pledge_penalty_for_continued_fault(
            reward_smoothed,
            quality_adj_power_smoothed,
            &result.previously_faulty_power.qa,
        );

        power_delta_total += &result.power_delta;
        pledge_delta_total += &result.pledge_delta;

        state
            .apply_penalty(&penalty_target)
            .map_err(|e| actor_error!(illegal_state, "failed to apply penalty: {}", e))?;

        log::debug!(
            "storage provider {} penalized {} for continued fault",
            rt.message().receiver(),
            penalty_target
        );

        let (penalty_from_vesting, penalty_from_balance) = state
            .repay_partial_debt_in_priority_order(
                rt.store(),
                rt.curr_epoch(),
                &rt.current_balance(),
            )
            .map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to unlock penalty")
            })?;

        penalty_total = &penalty_from_vesting + penalty_from_balance;
        pledge_delta_total -= penalty_from_vesting;

        continue_cron = state.continue_deadline_cron();
        if !continue_cron {
            state.deadline_cron_active = false;
        }

        Ok(state.clone())
    })?;

    // Remove power for new faults, and burn penalties.
    request_update_power(rt, power_delta_total)?;
    burn_funds(rt, penalty_total)?;
    notify_pledge_changed(rt, &pledge_delta_total)?;

    // Schedule cron callback for next deadline's last epoch.
    if continue_cron {
        let new_deadline_info = state.deadline_info(rt.policy(), curr_epoch + 1);
        enroll_cron_event(
            rt,
            new_deadline_info.last(),
            CronEventPayload {
                event_type: CRON_EVENT_PROVING_DEADLINE,
            },
        )?;
    } else {
        info!(
            "miner {} going inactive, deadline cron discontinued",
            rt.message().receiver()
        )
    }

    // Record whether or not we _have_ early terminations now.
    let has_early_terminations = have_pending_early_terminations(&state);

    // If we didn't have pending early terminations before, but we do now,
    // handle them at the next epoch.
    if !had_early_terminations && has_early_terminations {
        // First, try to process some of these terminations.
        if process_early_terminations(rt, reward_smoothed, quality_adj_power_smoothed)? {
            // If that doesn't work, just defer till the next epoch.
            schedule_early_termination_work(rt)?;
        }

        // Note: _don't_ process early terminations if we had a cron
        // callback already scheduled. In that case, we'll already have
        // processed AddressedSectorsMax terminations this epoch.
    }

    Ok(())
}

fn validate_expiration(
    policy: &Policy,
    curr_epoch: ChainEpoch,
    activation: ChainEpoch,
    expiration: ChainEpoch,
    seal_proof: RegisteredSealProof,
) -> Result<(), ActorError> {
    // Expiration must be after activation. Check this explicitly to avoid an underflow below.
    if expiration <= activation {
        return Err(actor_error!(
            illegal_argument,
            "sector expiration {} must be after activation {}",
            expiration,
            activation
        ));
    }

    // expiration cannot be less than minimum after activation
    if expiration - activation < policy.min_sector_expiration {
        return Err(actor_error!(
            illegal_argument,
            "invalid expiration {}, total sector lifetime ({}) must exceed {} after activation {}",
            expiration,
            expiration - activation,
            policy.min_sector_expiration,
            activation
        ));
    }

    // expiration cannot exceed MaxSectorExpirationExtension from now
    if expiration > curr_epoch + policy.max_sector_expiration_extension {
        return Err(actor_error!(
            illegal_argument,
            "invalid expiration {}, cannot be more than {} past current epoch {}",
            expiration,
            policy.max_sector_expiration_extension,
            curr_epoch
        ));
    }

    // total sector lifetime cannot exceed SectorMaximumLifetime for the sector's seal proof
    let max_lifetime = seal_proof_sector_maximum_lifetime(seal_proof).ok_or_else(|| {
        actor_error!(
            illegal_argument,
            "unrecognized seal proof type {:?}",
            seal_proof
        )
    })?;
    if expiration - activation > max_lifetime {
        return Err(actor_error!(
        illegal_argument,
        "invalid expiration {}, total sector lifetime ({}) cannot exceed {} after activation {}",
        expiration,
        expiration - activation,
        max_lifetime,
        activation
    ));
    }

    Ok(())
}

fn enroll_cron_event<BS, RT>(
    rt: &mut RT,
    event_epoch: ChainEpoch,
    cb: CronEventPayload,
) -> Result<(), ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let payload = serialize(&cb, "cron payload")?;
    let ser_params = serialize(
        &ext::power::EnrollCronEventParams {
            event_epoch,
            payload,
        },
        "cron params",
    )?;
    rt.send(
        &STORAGE_POWER_ACTOR_ADDR,
        ext::power::ENROLL_CRON_EVENT_METHOD,
        ser_params,
        TokenAmount::zero(),
    )?;

    Ok(())
}

fn request_update_power<BS, RT>(rt: &mut RT, delta: PowerPair) -> Result<(), ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    if delta.is_zero() {
        return Ok(());
    }

    let delta_clone = delta.clone();

    rt.send(
        &STORAGE_POWER_ACTOR_ADDR,
        ext::power::UPDATE_CLAIMED_POWER_METHOD,
        RawBytes::serialize(ext::power::UpdateClaimedPowerParams {
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
    BS: Blockstore,
    RT: Runtime<BS>,
{
    const MAX_LENGTH: usize = 8192;

    for chunk in deal_ids.chunks(MAX_LENGTH) {
        rt.send(
            &STORAGE_MARKET_ACTOR_ADDR,
            ext::market::ON_MINER_SECTORS_TERMINATE_METHOD,
            RawBytes::serialize(ext::market::OnMinerSectorsTerminateParamsRef {
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
    BS: Blockstore,
    RT: Runtime<BS>,
{
    info!("scheduling early terminations with cron...");
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

// returns true if valid, false if invalid, error if failed to validate either way!
fn verify_windowed_post<BS, RT>(
    rt: &RT,
    challenge_epoch: ChainEpoch,
    sectors: &[SectorOnChainInfo],
    proofs: Vec<PoStProof>,
) -> Result<bool, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let miner_actor_id: u64 = if let Payload::ID(i) = rt.message().receiver().payload() {
        *i
    } else {
        return Err(actor_error!(
            illegal_state,
            "runtime provided bad receiver address {}",
            rt.message().receiver()
        ));
    };

    // Regenerate challenge randomness, which must match that generated for the proof.
    let entropy = serialize(
        &rt.message().receiver(),
        "address for window post challenge",
    )?;
    let randomness = rt.get_randomness_from_beacon(
        DomainSeparationTag::WindowedPoStChallengeSeed,
        challenge_epoch,
        &entropy,
    )?;

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
        randomness: Randomness(randomness.into()),
        proofs,
        challenged_sectors,
        prover: miner_actor_id,
    };

    // verify the post proof
    let result = rt.verify_post(&pv_info);
    Ok(result.is_ok())
}

fn get_verify_info<BS, RT>(
    rt: &mut RT,
    params: SealVerifyParams,
    unsealed_cid: CompactCommD,
) -> Result<SealVerifyInfo, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    if rt.curr_epoch() <= params.interactive_epoch {
        return Err(actor_error!(forbidden, "too early to prove sector"));
    }

    let miner_actor_id: u64 = if let Payload::ID(i) = rt.message().receiver().payload() {
        *i
    } else {
        return Err(actor_error!(
            illegal_state,
            "runtime provided non ID receiver address {}",
            rt.message().receiver()
        ));
    };
    let entropy = serialize(&rt.message().receiver(), "address for get verify info")?;
    let randomness = rt.get_randomness_from_tickets(
        DomainSeparationTag::SealRandomness,
        params.seal_rand_epoch,
        &entropy,
    )?;
    let interactive_randomness = rt.get_randomness_from_beacon(
        DomainSeparationTag::InteractiveSealChallengeSeed,
        params.interactive_epoch,
        &entropy,
    )?;

    let commd = unsealed_cid.get_cid(params.registered_seal_proof)?;

    Ok(SealVerifyInfo {
        registered_proof: params.registered_seal_proof,
        sector_id: SectorID {
            miner: miner_actor_id,
            number: params.sector_num,
        },
        deal_ids: params.deal_ids,
        interactive_randomness: Randomness(interactive_randomness.into()),
        proof: params.proof,
        randomness: Randomness(randomness.into()),
        sealed_cid: params.sealed_cid,
        unsealed_cid: commd,
    })
}

fn request_deal_data<BS, RT>(
    rt: &mut RT,
    sectors: &[ext::market::SectorDeals],
) -> Result<ext::market::VerifyDealsForActivationReturn, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    // Short-circuit if there are no deals in any of the sectors.
    let mut deal_count = 0;
    for sector in sectors {
        deal_count += sector.deal_ids.len();
    }
    if deal_count == 0 {
        return Ok(ext::market::VerifyDealsForActivationReturn {
            sectors: vec![Default::default(); sectors.len()],
        });
    }

    let serialized = rt.send(
        &STORAGE_MARKET_ACTOR_ADDR,
        ext::market::VERIFY_DEALS_FOR_ACTIVATION_METHOD,
        RawBytes::serialize(ext::market::VerifyDealsForActivationParamsRef { sectors })?,
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
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let ret = rt
        .send(
            &REWARD_ACTOR_ADDR,
            ext::reward::THIS_EPOCH_REWARD_METHOD,
            Default::default(),
            TokenAmount::zero(),
        )
        .map_err(|e| e.wrap("failed to check epoch baseline power"))?;

    let ret: ThisEpochRewardReturn = deserialize(&ret, "epoch reward response")?;
    Ok(ret)
}

/// Requests the current network total power and pledge from the power actor.
fn request_current_total_power<BS, RT>(
    rt: &mut RT,
) -> Result<ext::power::CurrentTotalPowerReturn, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let ret = rt
        .send(
            &STORAGE_POWER_ACTOR_ADDR,
            ext::power::CURRENT_TOTAL_POWER_METHOD,
            Default::default(),
            TokenAmount::zero(),
        )
        .map_err(|e| e.wrap("failed to check current power"))?;

    let power: ext::power::CurrentTotalPowerReturn = deserialize(&ret, "total power response")?;
    Ok(power)
}

/// Resolves an address to an ID address and verifies that it is address of an account or multisig actor.
fn resolve_control_address<BS, RT>(rt: &RT, raw: Address) -> Result<Address, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let resolved = rt
        .resolve_address(&raw)
        .ok_or_else(|| actor_error!(illegal_argument, "unable to resolve address: {}", raw))?;

    let owner_code = rt
        .get_actor_code_cid(&resolved)
        .ok_or_else(|| actor_error!(illegal_argument, "no code for address: {}", resolved))?;

    let is_principal = rt
        .resolve_builtin_actor_type(&owner_code)
        .as_ref()
        .map(|t| CALLER_TYPES_SIGNABLE.contains(t))
        .unwrap_or(false);

    if !is_principal {
        return Err(actor_error!(
            illegal_argument,
            "owner actor type must be a principal, was {}",
            owner_code
        ));
    }

    Ok(Address::new_id(resolved))
}

/// Resolves an address to an ID address and verifies that it is address of an account actor with an associated BLS key.
/// The worker must be BLS since the worker key will be used alongside a BLS-VRF.
fn resolve_worker_address<BS, RT>(rt: &mut RT, raw: Address) -> Result<Address, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let resolved = rt
        .resolve_address(&raw)
        .ok_or_else(|| actor_error!(illegal_argument, "unable to resolve address: {}", raw))?;

    let worker_code = rt
        .get_actor_code_cid(&resolved)
        .ok_or_else(|| actor_error!(illegal_argument, "no code for address: {}", resolved))?;
    if rt.resolve_builtin_actor_type(&worker_code) != Some(Type::Account) {
        return Err(actor_error!(
            illegal_argument,
            "worker actor type must be an account, was {}",
            worker_code
        ));
    }

    if raw.protocol() != Protocol::BLS {
        let ret = rt.send(
            &Address::new_id(resolved),
            ext::account::PUBKEY_ADDRESS_METHOD,
            RawBytes::default(),
            TokenAmount::zero(),
        )?;
        let pub_key: Address = deserialize(&ret, "address response")?;
        if pub_key.protocol() != Protocol::BLS {
            return Err(actor_error!(
                illegal_argument,
                "worker account {} must have BLS pubkey, was {}",
                resolved,
                pub_key.protocol()
            ));
        }
    }
    Ok(Address::new_id(resolved))
}

fn burn_funds<BS, RT>(rt: &mut RT, amount: TokenAmount) -> Result<(), ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    log::debug!(
        "storage provder {} burning {}",
        rt.message().receiver(),
        amount
    );
    if amount.is_positive() {
        rt.send(
            &BURNT_FUNDS_ACTOR_ADDR,
            METHOD_SEND,
            RawBytes::default(),
            amount,
        )?;
    }
    Ok(())
}

fn notify_pledge_changed<BS, RT>(rt: &mut RT, pledge_delta: &TokenAmount) -> Result<(), ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    if !pledge_delta.is_zero() {
        rt.send(
            &STORAGE_POWER_ACTOR_ADDR,
            ext::power::UPDATE_PLEDGE_TOTAL_METHOD,
            RawBytes::serialize(pledge_delta)?,
            TokenAmount::zero(),
        )?;
    }
    Ok(())
}

fn get_claims<BS, RT>(
    rt: &mut RT,
    ids: &Vec<ext::verifreg::ClaimID>,
) -> Result<Vec<ext::verifreg::Claim>, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let params = ext::verifreg::GetClaimsParams {
        provider: rt.message().receiver().id().unwrap(),
        claim_ids: ids.clone(),
    };
    let ret_raw = rt.send(
        &VERIFIED_REGISTRY_ACTOR_ADDR,
        ext::verifreg::GET_CLAIMS_METHOD as u64,
        serialize(&params, "get claims parameters")?,
        TokenAmount::zero(),
    )?;
    let claims_ret: ext::verifreg::GetClaimsReturn = deserialize(&ret_raw, "get claims return")?;
    if (claims_ret.batch_info.success_count as usize) < ids.len() {
        return Err(actor_error!(illegal_argument, "invalid claims"));
    }
    Ok(claims_ret.claims)
}

/// Assigns proving period offset randomly in the range [0, WPoStProvingPeriod) by hashing
/// the actor's address and current epoch.
fn assign_proving_period_offset(
    policy: &Policy,
    addr: Address,
    current_epoch: ChainEpoch,
    blake2b: impl FnOnce(&[u8]) -> [u8; 32],
) -> anyhow::Result<ChainEpoch> {
    let mut my_addr = addr.marshal_cbor()?;
    my_addr.write_i64::<BigEndian>(current_epoch)?;

    let digest = blake2b(&my_addr);

    let mut offset: u64 = BigEndian::read_u64(&digest);
    offset %= policy.wpost_proving_period as u64;

    // Conversion from i64 to u64 is safe because it's % WPOST_PROVING_PERIOD which is i64
    Ok(offset as ChainEpoch)
}

/// Computes the epoch at which a proving period should start such that it is greater than the current epoch, and
/// has a defined offset from being an exact multiple of WPoStProvingPeriod.
/// A miner is exempt from Winow PoSt until the first full proving period starts.
fn current_proving_period_start(
    policy: &Policy,
    current_epoch: ChainEpoch,
    offset: ChainEpoch,
) -> ChainEpoch {
    let curr_modulus = current_epoch % policy.wpost_proving_period;

    let period_progress = if curr_modulus >= offset {
        curr_modulus - offset
    } else {
        policy.wpost_proving_period - (offset - curr_modulus)
    };

    current_epoch - period_progress
}

fn current_deadline_index(
    policy: &Policy,
    current_epoch: ChainEpoch,
    period_start: ChainEpoch,
) -> u64 {
    ((current_epoch - period_start) / policy.wpost_challenge_window) as u64
}

/// Computes deadline information for a fault or recovery declaration.
/// If the deadline has not yet elapsed, the declaration is taken as being for the current proving period.
/// If the deadline has elapsed, it's instead taken as being for the next proving period after the current epoch.
fn declaration_deadline_info(
    policy: &Policy,
    period_start: ChainEpoch,
    deadline_idx: u64,
    current_epoch: ChainEpoch,
) -> anyhow::Result<DeadlineInfo> {
    if deadline_idx >= policy.wpost_period_deadlines {
        return Err(anyhow!(
            "invalid deadline {}, must be < {}",
            deadline_idx,
            policy.wpost_period_deadlines
        ));
    }

    let deadline =
        new_deadline_info(policy, period_start, deadline_idx, current_epoch).next_not_elapsed();
    Ok(deadline)
}

/// Checks that a fault or recovery declaration at a specific deadline is outside the exclusion window for the deadline.
fn validate_fr_declaration_deadline(deadline: &DeadlineInfo) -> anyhow::Result<()> {
    if deadline.fault_cutoff_passed() {
        Err(anyhow!("late fault or recovery declaration"))
    } else {
        Ok(())
    }
}

/// Validates that a partition contains the given sectors.
fn validate_partition_contains_sectors(
    partition: &Partition,
    sectors: &BitField,
) -> anyhow::Result<()> {
    // Check that the declared sectors are actually assigned to the partition.
    if partition.sectors.contains_all(sectors) {
        Ok(())
    } else {
        Err(anyhow!("not all sectors are assigned to the partition"))
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

pub fn power_for_sector(sector_size: SectorSize, sector: &SectorOnChainInfo) -> PowerPair {
    PowerPair {
        raw: BigInt::from(sector_size as u64),
        qa: qa_power_for_sector(sector_size, sector),
    }
}

/// Returns the sum of the raw byte and quality-adjusted power for sectors.
pub fn power_for_sectors(sector_size: SectorSize, sectors: &[SectorOnChainInfo]) -> PowerPair {
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
    BS: Blockstore,
{
    state
        .get_info(store)
        .map_err(|e| e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "could not read miner info"))
}

fn process_pending_worker<BS, RT>(
    info: &mut MinerInfo,
    rt: &RT,
    state: &mut State,
) -> Result<(), ActorError>
where
    BS: Blockstore,
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
        .save_info(rt.store(), info)
        .map_err(|e| e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to save miner info"))
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
    BS: Blockstore,
    RT: Runtime<BS>,
{
    let res = state.repay_debts(&rt.current_balance()).map_err(|e| {
        e.downcast_default(
            ExitCode::USR_ILLEGAL_STATE,
            "unlocked balance can not repay fee debt",
        )
    })?;
    info!("RepayDebtsOrAbort was called and succeeded");
    Ok(res)
}

fn check_control_addresses(policy: &Policy, control_addrs: &[Address]) -> Result<(), ActorError> {
    if control_addrs.len() > policy.max_control_addresses {
        return Err(actor_error!(
            illegal_argument,
            "control addresses length {} exceeds max control addresses length {}",
            control_addrs.len(),
            policy.max_control_addresses
        ));
    }

    Ok(())
}

fn check_valid_post_proof_type(
    policy: &Policy,
    proof_type: RegisteredPoStProof,
) -> Result<(), ActorError> {
    if policy.valid_post_proof_type.contains(&proof_type) {
        Ok(())
    } else {
        Err(actor_error!(
            illegal_argument,
            "proof type {:?} not allowed for new miner actors",
            proof_type
        ))
    }
}

fn check_peer_info(
    policy: &Policy,
    peer_id: &[u8],
    multiaddrs: &[BytesDe],
) -> Result<(), ActorError> {
    if peer_id.len() > policy.max_peer_id_length {
        return Err(actor_error!(
            illegal_argument,
            "peer ID size of {} exceeds maximum size of {}",
            peer_id.len(),
            policy.max_peer_id_length
        ));
    }

    let mut total_size = 0;
    for ma in multiaddrs {
        if ma.0.is_empty() {
            return Err(actor_error!(illegal_argument, "invalid empty multiaddr"));
        }
        total_size += ma.0.len();
    }

    if total_size > policy.max_multiaddr_data {
        return Err(actor_error!(
            illegal_argument,
            "multiaddr size of {} exceeds maximum of {}",
            total_size,
            policy.max_multiaddr_data
        ));
    }

    Ok(())
}

fn confirm_sector_proofs_valid_internal<BS, RT>(
    rt: &mut RT,
    pre_commits: Vec<SectorPreCommitOnChainInfo>,
    this_epoch_baseline_power: &BigInt,
    this_epoch_reward_smoothed: &FilterEstimate,
    quality_adj_power_smoothed: &FilterEstimate,
) -> Result<(), ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    // get network stats from other actors
    let circulating_supply = rt.total_fil_circ_supply();

    // Ideally, we'd combine some of these operations, but at least we have
    // a constant number of them.
    let activation = rt.curr_epoch();
    // Pre-commits for new sectors.
    let mut valid_pre_commits = Vec::default();

    for pre_commit in pre_commits {
        match activate_deals_and_claim_allocations(
            rt,
            pre_commit.clone().info.deal_ids,
            pre_commit.info.expiration,
            pre_commit.info.sector_number,
        )? {
            None => {
                info!(
                    "failed to activate deals on sector {}, dropping from prove commit set",
                    pre_commit.info.sector_number,
                );
                continue;
            }
            Some(deal_spaces) => valid_pre_commits.push((pre_commit, deal_spaces)),
        };
    }

    // When all prove commits have failed abort early
    if valid_pre_commits.is_empty() {
        return Err(actor_error!(
            illegal_argument,
            "all prove commits failed to validate"
        ));
    }

    let (total_pledge, newly_vested) = rt.transaction(|state: &mut State, rt| {
        let policy = rt.policy();
        let store = rt.store();
        let info = get_miner_info(store, state)?;

        let mut new_sector_numbers = Vec::<SectorNumber>::with_capacity(valid_pre_commits.len());
        let mut deposit_to_unlock = TokenAmount::zero();
        let mut new_sectors = Vec::<SectorOnChainInfo>::new();
        let mut total_pledge = TokenAmount::zero();

        for (pre_commit, deal_spaces) in valid_pre_commits {
            // compute initial pledge
            let duration = pre_commit.info.expiration - activation;

            // This should have been caught in precommit, but don't let other sectors fail because of it.
            if duration < policy.min_sector_expiration {
                warn!(
                    "precommit {} has lifetime {} less than minimum {}. ignoring",
                    pre_commit.info.sector_number, duration, policy.min_sector_expiration,
                );
                continue;
            }

            let deal_weight = deal_spaces.deal_space * duration;
            let verified_deal_weight = deal_spaces.verified_deal_space * duration;

            let power = qa_power_for_weight(
                info.sector_size,
                duration,
                &deal_weight,
                &verified_deal_weight,
            );

            let day_reward = expected_reward_for_power(
                this_epoch_reward_smoothed,
                quality_adj_power_smoothed,
                &power,
                fil_actors_runtime_v9::EPOCHS_IN_DAY,
            );

            // The storage pledge is recorded for use in computing the penalty if this sector is terminated
            // before its declared expiration.
            // It's not capped to 1 FIL, so can exceed the actual initial pledge requirement.
            let storage_pledge = expected_reward_for_power(
                this_epoch_reward_smoothed,
                quality_adj_power_smoothed,
                &power,
                INITIAL_PLEDGE_PROJECTION_PERIOD,
            );

            let initial_pledge = initial_pledge_for_power(
                &power,
                this_epoch_baseline_power,
                this_epoch_reward_smoothed,
                quality_adj_power_smoothed,
                &circulating_supply,
            );

            deposit_to_unlock += &pre_commit.pre_commit_deposit;
            total_pledge += &initial_pledge;

            let new_sector_info = SectorOnChainInfo {
                sector_number: pre_commit.info.sector_number,
                seal_proof: pre_commit.info.seal_proof,
                sealed_cid: pre_commit.info.sealed_cid,
                deal_ids: pre_commit.info.deal_ids,
                expiration: pre_commit.info.expiration,
                activation,
                deal_weight,
                verified_deal_weight,
                initial_pledge,
                expected_day_reward: day_reward,
                expected_storage_pledge: storage_pledge,
                replaced_sector_age: ChainEpoch::zero(),
                replaced_day_reward: TokenAmount::zero(),
                sector_key_cid: None,
                simple_qa_power: true,
            };

            new_sector_numbers.push(new_sector_info.sector_number);
            new_sectors.push(new_sector_info);
        }

        state.put_sectors(store, new_sectors.clone()).map_err(|e| {
            e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to put new sectors")
        })?;

        state
            .delete_precommitted_sectors(store, &new_sector_numbers)
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::USR_ILLEGAL_STATE,
                    "failed to delete precommited sectors",
                )
            })?;

        state
            .assign_sectors_to_deadlines(
                policy,
                store,
                rt.curr_epoch(),
                new_sectors,
                info.window_post_partition_sectors,
                info.sector_size,
            )
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::USR_ILLEGAL_STATE,
                    "failed to assign new sectors to deadlines",
                )
            })?;

        let newly_vested = TokenAmount::zero();

        // Unlock deposit for successful proofs, make it available for lock-up as initial pledge.
        state
            .add_pre_commit_deposit(&(-deposit_to_unlock))
            .map_err(|e| actor_error!(illegal_state, "failed to add precommit deposit: {}", e))?;

        let unlocked_balance = state
            .get_unlocked_balance(&rt.current_balance())
            .map_err(|e| {
                actor_error!(illegal_state, "failed to calculate unlocked balance: {}", e)
            })?;
        if unlocked_balance < total_pledge {
            return Err(actor_error!(
                insufficient_funds,
                "insufficient funds for aggregate initial pledge requirement {}, available: {}",
                total_pledge,
                unlocked_balance
            ));
        }

        state
            .add_initial_pledge(&total_pledge)
            .map_err(|e| actor_error!(illegal_state, "failed to add initial pledge: {}", e))?;

        state
            .check_balance_invariants(&rt.current_balance())
            .map_err(balance_invariants_broken)?;

        Ok((total_pledge, newly_vested))
    })?;

    // Request pledge update for activated sector.
    notify_pledge_changed(rt, &(total_pledge - newly_vested))?;

    Ok(())
}

// activate deals with builtin market and claim allocations with verified registry actor
// returns an error in case of a fatal programmer error
// returns Ok(None) in case deal activation or verified allocation claim fails
fn activate_deals_and_claim_allocations<RT, BS>(
    rt: &mut RT,
    deal_ids: Vec<DealID>,
    sector_expiry: ChainEpoch,
    sector_number: SectorNumber,
) -> Result<Option<crate::ext::market::DealSpaces>, ActorError>
where
    BS: Blockstore,
    RT: Runtime<BS>,
{
    if deal_ids.is_empty() {
        return Ok(Some(ext::market::DealSpaces::default()));
    }
    // Check (and activate) storage deals associated to sector. Abort if checks failed.
    let activate_raw = rt.send(
        &STORAGE_MARKET_ACTOR_ADDR,
        ext::market::ACTIVATE_DEALS_METHOD,
        RawBytes::serialize(ext::market::ActivateDealsParams {
            deal_ids,
            sector_expiry,
        })?,
        TokenAmount::zero(),
    );
    let activate_res: ext::market::ActivateDealsResult = match activate_raw {
        Ok(res) => res.deserialize()?,
        Err(e) => {
            info!(
                "error activating deals on sector {}: {}",
                sector_number,
                e.msg()
            );
            return Ok(None);
        }
    };

    // If deal activation includes verified deals claim allocations
    if activate_res.verified_infos.is_empty() {
        return Ok(Some(ext::market::DealSpaces {
            deal_space: activate_res.nonverified_deal_space,
            ..Default::default()
        }));
    }
    let sector_claims = activate_res
        .verified_infos
        .iter()
        .map(|info| ext::verifreg::SectorAllocationClaim {
            client: info.client,
            allocation_id: info.allocation_id,
            data: info.data,
            size: info.size,
            sector: sector_number,
            sector_expiry,
        })
        .collect();

    let claim_raw = rt.send(
        &VERIFIED_REGISTRY_ACTOR_ADDR,
        ext::verifreg::CLAIM_ALLOCATIONS_METHOD,
        RawBytes::serialize(ext::verifreg::ClaimAllocationsParams {
            sectors: sector_claims,
            all_or_nothing: true,
        })?,
        TokenAmount::zero(),
    );
    let claim_res: ext::verifreg::ClaimAllocationsReturn = match claim_raw {
        Ok(res) => res.deserialize()?,
        Err(e) => {
            info!(
                "error claiming allocation on sector {}: {}",
                sector_number,
                e.msg()
            );
            return Ok(None);
        }
    };
    Ok(Some(ext::market::DealSpaces {
        deal_space: activate_res.nonverified_deal_space,
        verified_deal_space: claim_res.claimed_space,
    }))
}

// XXX: probably better to push this one level down into state
fn balance_invariants_broken(e: Error) -> ActorError {
    ActorError::unchecked(
        ERR_BALANCE_INVARIANTS_BROKEN,
        format!("balance invariants broken: {}", e),
    )
}

impl ActorCode for Actor {
    fn invoke_method<BS, RT>(
        rt: &mut RT,
        method: MethodNum,
        params: &RawBytes,
    ) -> Result<RawBytes, ActorError>
    where
        BS: Blockstore + Clone,
        RT: Runtime<BS>,
    {
        match FromPrimitive::from_u64(method) {
            Some(Method::Constructor) => {
                Self::constructor(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::ControlAddresses) => {
                let res = Self::control_addresses(rt)?;
                Ok(RawBytes::serialize(&res)?)
            }
            Some(Method::ChangeWorkerAddress) => {
                Self::change_worker_address(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::ChangePeerID) => {
                Self::change_peer_id(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::SubmitWindowedPoSt) => {
                Self::submit_windowed_post(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::PreCommitSector) => {
                Self::pre_commit_sector(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::ProveCommitSector) => {
                Self::prove_commit_sector(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::ExtendSectorExpiration) => {
                Self::extend_sector_expiration(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::TerminateSectors) => {
                let ret = Self::terminate_sectors(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::serialize(ret)?)
            }
            Some(Method::DeclareFaults) => {
                Self::declare_faults(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::DeclareFaultsRecovered) => {
                Self::declare_faults_recovered(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::OnDeferredCronEvent) => {
                Self::on_deferred_cron_event(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::CheckSectorProven) => {
                Self::check_sector_proven(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::ApplyRewards) => {
                Self::apply_rewards(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::ReportConsensusFault) => {
                Self::report_consensus_fault(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::WithdrawBalance) => {
                let res = Self::withdraw_balance(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::serialize(&res)?)
            }
            Some(Method::ConfirmSectorProofsValid) => {
                Self::confirm_sector_proofs_valid(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::ChangeMultiaddrs) => {
                Self::change_multiaddresses(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::CompactPartitions) => {
                Self::compact_partitions(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::CompactSectorNumbers) => {
                Self::compact_sector_numbers(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::ConfirmUpdateWorkerKey) => {
                Self::confirm_update_worker_key(rt)?;
                Ok(RawBytes::default())
            }
            Some(Method::RepayDebt) => {
                Self::repay_debt(rt)?;
                Ok(RawBytes::default())
            }
            Some(Method::ChangeOwnerAddress) => {
                Self::change_owner_address(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::DisputeWindowedPoSt) => {
                Self::dispute_windowed_post(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::PreCommitSectorBatch) => {
                Self::pre_commit_sector_batch(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::ProveCommitAggregate) => {
                Self::prove_commit_aggregate(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::ProveReplicaUpdates) => {
                let res = Self::prove_replica_updates(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::serialize(res)?)
            }
            Some(Method::PreCommitSectorBatch2) => {
                Self::pre_commit_sector_batch2(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::ProveReplicaUpdates2) => {
                let res = Self::prove_replica_updates2(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::serialize(res)?)
            }
            Some(Method::ChangeBeneficiary) => {
                Self::change_beneficiary(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            Some(Method::GetBeneficiary) => {
                let res = Self::get_beneficiary(rt)?;
                Ok(RawBytes::serialize(res)?)
            }
            Some(Method::ExtendSectorExpiration2) => {
                Self::extend_sector_expiration2(rt, cbor::deserialize_params(params)?)?;
                Ok(RawBytes::default())
            }
            None => Err(actor_error!(unhandled_message, "Invalid method")),
        }
    }
}

#[cfg(test)]
mod internal_tests;
