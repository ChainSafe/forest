// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(unused_variables)]
#![allow(dead_code)]

mod deadlines;
mod policy;
mod state;
mod types;

pub use self::deadlines::*;
pub use self::policy::*;
pub use self::state::*;
pub use self::types::*;
use crate::{CALLER_TYPES_SIGNABLE, INIT_ACTOR_ADDR, STORAGE_POWER_ACTOR_ADDR,STORAGE_MARKET_ACTOR_ADDR, REWARD_ACTOR_ADDR, BURNT_FUNDS_ACTOR_ADDR, OptionalEpoch,check_empty_params, make_map};
use crate::power::{OnSectorModifyWeightDescParams, SectorTermination, OnSectorProveCommitParams, SectorStorageWeightDesc, Method as PowerMethod, SECTOR_TERMINATION_MANUAL, SECTOR_TERMINATION_EXPIRED, SECTOR_TERMINATION_FAULTY};
use crate::market::{VerifyDealsOnSectorProveCommitParams, VerifyDealsOnSectorProveCommitReturn, Method as MarketMethod};
use address::Address;
use cid::{Cid, multihash::Blake2b256};
use crypto::DomainSeparationTag::WindowPoStDeadlineAssignment;
use clock::ChainEpoch;
use fil_types::{SealVerifyParams, PoStProof, RegisteredProof, SectorSize, SectorNumber};
use ipld_blockstore::BlockStore;
use ipld_amt::Amt;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Zero};
use runtime::{ActorCode, Runtime, Syscalls};
use vm::{ActorError, DealID, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR, METHOD_SEND};
use message::Message;
use std::collections::HashMap;
use num_bigint::BigUint;
use num_bigint::biguint_ser::{BigUintDe, BigUintSer};
use bitfield::BitField;

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

        // Sanity check that we've been given a valid peer ID
        // TODO

        if !check_supported_proof_types(params.seal_proof_type) {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!("proof type {:?} not allowed for new miner actors", params.seal_proof_type),
            );
        };

        let owner = resolve_owner_address(rt, params.owner)?;
        let worker = resolve_worker_address(rt, params.worker)?;

        let empty_map = make_map(rt.store()).flush().map_err(|err| {
            rt.abort(
                ExitCode::ErrIllegalState,
                format!("Failed to construct miner state: {}", err),
            )
        })?;

        let empty_root = Amt::<Cid, BS>::new(rt.store()).flush().map_err(|e| {
            rt.abort(
                ExitCode::ErrIllegalState,
                format!("Failed to construct miner state: {}", e),
            )
        })?;

        let empty_deadlines = Deadlines::new();
        // TODO
        let empty_deadlines_cid = rt.store().put(&empty_deadlines, Blake2b256).unwrap();

        let current_epoch = rt.curr_epoch();
        let offset = assign_proving_period_offset(*rt.message().to(), current_epoch, rt.syscalls())?;
        // TODO
        let period_start = next_proving_period_start(current_epoch, offset).unwrap();
        assert!(period_start > current_epoch);  
        // TODO handle actor error potential
        let st = State::new(empty_root, empty_map, empty_deadlines_cid, owner, worker, params.peer_id, params.seal_proof_type);
        rt.create(&st)?;

        // Register cron callback for epoch before the first proving period starts.
        enroll_cron_event(rt, period_start - 1, CronEventPayload{
            event_type: CRON_EVENT_PROVING_PERIOD,
            sectors: BitField::default()
        });

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
        Ok(GetControlAddressesReturn{
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
            sectors: BitField::default()
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
                    format!("too many partitions {}, limit {}", params.partitions.len(), submission_partition_limit),
                );
            }
            // TODO ask about none case
            let deadline = match st.deadline_info(current_epoch) {
                Some(deadline) => deadline,
                None => return Err(ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("no deadline info found for epoch {}", current_epoch),
                )),
            }; 

            if !deadline.period_start() {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("proving period {} not yet open at {}", deadline.period_start, current_epoch),
                );
            }
            if deadline.period_elapsed() {
                // A cron event has not yet processed the previous proving period and established the next one.
			    // This is possible in the first non-empty epoch of a proving period if there was an empty tipset on the
                // last epoch of the previous period.
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("proving period at {} elapsed, next one not yet opened", deadline.period_start),
                );
            }
            if params.deadline != deadline.index {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("invalid deadline {} at epoch {}, expected {}", params.deadline, current_epoch, deadline.index),
                );
            }
            // Verify locked funds are are at least the sum of sector initial pledges.
		    // Note that this call does not actually compute recent vesting, so the reported locked funds may be
		    // slightly higher than the true amount (i.e. slightly in the miner's favour).
		    // Computing vesting here would be almost always redundant since vesting is quantized to ~daily units.
            // Vesting will be at most one proving period old if computed in the cron callback.
            verify_pledge_meets_initial_requirements(rt, &st);

            let deadlines = st.load_deadlines(rt.store()).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to load deadlines")
                )
            })?;

            // Traverse earlier submissions and enact detected faults.
		    // This isn't strictly necessary, but keeps the power table up to date eagerly and can force payment
            // of penalties if locked pledge drops too low.
            let (detected_faults, p) = check_missing_post_faults(rt, rt.store(), &deadlines, &deadline.period_start, &deadline.index, &current_epoch)?;
            detected_faults_sector = detected_faults;
            penalty = p;
            // TODO WPOST (follow-up): process Skipped as faults

            // Work out which sectors are due in the declared partitions at this deadline.
            let partitions_sectors = compute_partitions_sector(&deadlines, partitions_size, deadline.index, &params.partitions)?;
            // TODO BITFIELD Union
            let proven_sectors = BitField::default();
            // TODO handle actor err

            let (sector_infos, mut declared_recoveries) = st.load_sector_infos_for_proof(rt.store(), proven_sectors)?;
            
            // Verify the proof.
            // A failed verification doesn't immediately cause a penalty; the miner can try again.
            verify_windowed_post(rt, &deadline.challenge, &sector_infos, &params.proofs);

            // Record the successful submission
            let post_partitions: BitVec<Lsb0, u64> = BitVec::from_vec(params.partitions.clone());
            // TODO bitfield contains any...
            // TODO check actor err

            // If the PoSt was successful, the declared recoveries should be restored
            st.remove_faults(rt.store(), &declared_recoveries)?;

            st.remove_recoveries(&declared_recoveries)?;

            // Load info for recovered sectors for recovery of power outside this state transaction.
            let empty = declared_recoveries.is_empty();
            // TODO deal with actor error

            if !empty {
                let _sectors_by_number: HashMap<SectorNumber, SectorOnChainInfo> = HashMap::default();
                // TODO FIX
                // for s in sector_infos {
                //     sectors_by_number[&s.info.sector_number] = s;
                // }
                // declared_recoveries.for_each(|i, _| {
                //     let key : SectorNumber = i as u64;
                //     recovered_sectors.push(sectors_by_number[&key]);
                //     true // TODO ask
                // })
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
                format!("sector expiration {} must be after now {}", params.expiration, rt.curr_epoch()),
            );
        }
        if params.seal_rand_epoch >= rt.curr_epoch() {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!("seal challenge epoch {} must be before now {}", params.seal_rand_epoch, rt.curr_epoch()),
            );
        }
        let challenge_earliest = seal_challenge_earliest(rt.curr_epoch(), params.registered_proof);
        if params.seal_rand_epoch < challenge_earliest {
            // The subsequent commitment proof can't possibly be accepted because the seal challenge will be deemed
		    // too old. Note that passing this check doesn't guarantee the proof will be soon enough, depending on
            // when it arrives. 
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!("seal challenge epoch {} too old, must be after {}", params.seal_rand_epoch, challenge_earliest),
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
            
            let precommit = st.get_precommitted_sector(rt.store(), params.sector_number).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to check precommitted sector: {}, {}", sector_number, e),
                )
                })?;
             
            if precommit.is_some() {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("sector {} already precommitted", sector_number),
                )
            };    

            if st.has_sector_number(rt.store(), params.sector_number).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to check sector: {}, {}", sector_number, e),
                )
                })? {
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
            };

            if st.proving_period_start.is_none() {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("proving period is none: {:?}", st.proving_period_start)
                );
            };

            // Check expiry is exactly *the epoch before* the start of a proving period.
            let period_offset = st.proving_period_start.unwrap() % WPOST_PROVING_PERIOD;
            let expiry_offset = (params.expiration + 1) % WPOST_PROVING_PERIOD;
            if expiry_offset != period_offset {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("invalid expiration {}, must be immediately before proving period boundary {} mod {}", params.expiration, period_offset, WPOST_PROVING_PERIOD),
                );
            }
            // TODO specs actors currently not handeling this err
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
                    format!("failed to write pre-committed sector: {}, {}", sector_number, e),
                )
            })?;
            Ok(newly_vested_amount)
        })??;

        notify_pledge_change(rt, &newly_vested_amount);
        let mut bf = BitField::new();
        bf.set_elements(params.sector_number as u8);

        // Request deferred Cron check for PreCommit expiry check.
        let cron_payload = CronEventPayload{
            event_type: CRON_EVENT_PRE_COMMIT_EXPIRY,
            sectors: bf,
        };

        let msd = max_seal_duration(&params.registered_proof)?;
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

        let precommit = st.get_precommitted_sector(rt.store(), sector_number).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("failed to get precommitted sector: {}, {}", sector_number, e),
            )
        })?.ok_or_else(|| {
            ActorError::new(
                ExitCode::ErrNotFound,
                format!("no precommitted sector: {}", sector_number),
            )
        })?;
 
        let msd = max_seal_duration(&precommit.info.registered_proof)?;
        let prove_commit_due = precommit.pre_commit_epoch + msd;
        if rt.curr_epoch() > prove_commit_due {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!("commitment proof for {} too late at {}, due {}", sector_number, rt.curr_epoch(), prove_commit_due),
            );
        }
        
        // will abort if seal invalid
        verify_seal(rt, &SealVerifyStuff {
            sealed_cid: precommit.info.sealed_cid.clone(),
            interactive_epoch: precommit.pre_commit_epoch + PRE_COMMIT_CHALLENGE_DELAY,
            seal_rand_epoch: precommit.info.seal_rand_epoch,
            proof: params.proof,
            deal_ids: precommit.info.deal_ids.clone(),
            sector_num: precommit.info.sector_number,
            registered_proof: precommit.info.registered_proof
        });

        // Check (and activate) storage deals associated to sector. Abort if checks failed.
        // return DealWeight for the deal set in the sector
        let mut method_num = MarketMethod::VerifyDealsOnSectorProveCommit as u64; // todo ask
        let mut ser_params = Serialized::serialize(&VerifyDealsOnSectorProveCommitParams{
            deal_ids: precommit.info.deal_ids.clone(),
            sector_expiry: precommit.info.expiration
        })?;
        let mut ret = rt.send(&*STORAGE_MARKET_ACTOR_ADDR, method_num, &ser_params, &BigUint::zero())?;
        // TODO ask about assertNoErr and require success
        let deal_weights: VerifyDealsOnSectorProveCommitReturn = ret.deserialize()?; 


        // Request power for activated sector.
        // Return initial pledge requirement.
        method_num = PowerMethod::OnSectorProveCommit as u64;
        ser_params = Serialized::serialize(OnSectorProveCommitParams{
            weight: SectorStorageWeightDesc {
                sector_size: st.info.sector_size,
                deal_weight: deal_weights.deal_weight.clone(),
                verified_deal_weight: deal_weights.verified_deal_weight.clone(),
                duration: precommit.info.expiration - rt.curr_epoch()
            }
        })?;
        ret = rt.send(&*STORAGE_MARKET_ACTOR_ADDR, method_num, &ser_params, &BigUint::zero())?;
        // TODO assert no error check
        let BigUintDe(initial_pledge) = ret.deserialize()?;
        
        // Add sector and pledge lock-up to miner state
        let newly_vested_amount = rt.transaction::<State, Result<TokenAmount, ActorError>, _>(|st: &mut State, rt| {
           let vested_amount = st.unlock_vested_funds(rt.store(), rt.curr_epoch()).map_err(|e| {
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
                    format!("insufficient funds for initial pledge requirement {}, available: {}", initial_pledge, available_balance),
                );
            }
            st.add_locked_funds(rt.store(), &rt.curr_epoch(), &initial_pledge, PLEDGE_VESTING_SPEC).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to add pledge: {}", e),
                )})?;

            st.assert_balance_invariants(&rt.current_balance()?);

            let new_sector_info = SectorOnChainInfo{
                info: precommit.info.clone(),
                activation_epoch: rt.curr_epoch(),
                deal_weight: deal_weights.deal_weight,
                verified_deal_weight: deal_weights.verified_deal_weight
            };  
            
            st.put_sector(rt.store(), new_sector_info).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to prove commit: {}", e),
                )})?;

            st.delete_precommitted_sector(rt.store(), sector_number).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to delete precommit for sector: {}, {}", sector_number, e),
                )})?;
            
            st.add_sector_expirations(rt.store(), &precommit.info.expiration, &[sector_number]).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to add new sector {} expiration: {}", sector_number, e),
                )})?;

            // Add to new sectors, a staging ground before scheduling to a deadline at end of proving period.
            st.add_new_sectors(&[sector_number]).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to add new sector number {}: {}", sector_number, e),
                )})?;

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
    
        let sec = st.get_sector(rt.store(), params.sector_number).map_err(|e| {
            ActorError::new(
                ExitCode::ErrNotFound,
                format!("Sector hasn't been proven: {}, {}", params.sector_number, e),
            )})?;
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
        
        let mut sector = match st.get_sector(rt.store(), params.sector_number).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("failed to add load sector {}: {}", params.sector_number, e),
            )})? {
                Some(sector) => sector,
                None => return Err(ActorError::new(
                    ExitCode::ErrNotFound,
                    format!("no such sector {}", params.sector_number),
                )),
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

        let ret = rt.send(&*STORAGE_POWER_ACTOR_ADDR, method_num, &ser_params, &BigUint::zero())?;
        // TODO ask about require_success

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
        params: TerminateSectorsParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let st : State = rt.state()?;
        rt.validate_immediate_caller_is(std::iter::once(&st.info.worker))?;

        // Note: this cannot terminate pre-committed but un-proven sectors.
        // They must be allowed to expire (and deposit burnt).
        terminate_sectors(rt, params.sectors, SECTOR_TERMINATION_MANUAL);
        Ok(())
    }

    ////////////
    // Faults //
    ////////////
    
    // TODO finish bitfield operations
    fn declare_faults<BS, RT>(rt: &mut RT, params: DeclareFaultsParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        if params.faults.len() as u64 > WPOST_PERIOD_DEADLINES {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!("too many declarations {}, max {}", params.faults.len(), WPOST_PERIOD_DEADLINES),
            );
        }
        let current_epoch = rt.curr_epoch();
        let declared_fault_sectors : Vec<SectorOnChainInfo> = Vec::new();
        let mut detected_fault_sectors : Vec<SectorOnChainInfo> = Vec::new();
        let mut penalty = BigUint::zero();

        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            rt.validate_immediate_caller_is(std::iter::once(&st.info.worker))?;
            
             // TODO ask about none case
             let current_deadline = match st.deadline_info(current_epoch) {
                Some(current_deadline) => current_deadline,
                None => return Err(ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("no deadline info found for epoch {}", current_epoch),
                )),
            };
            let deadlines = st.load_deadlines(rt.store()).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("Failed to load deadlines: {}",  e),
                )
            })?;

            // Traverse earlier submissions and enact detected faults.
		    // This is necessary to prevent the miner "declaring" a fault for a PoSt already missed.
            let (detected_faults, p) = check_missing_post_faults(rt, rt.store(), &deadlines, &current_deadline.period_start, &current_deadline.index, &current_epoch)?;
            detected_fault_sectors = detected_faults;
            penalty = p;

            if st.proving_period_start.is_none() {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("proving period is none: {:?}", st.proving_period_start)
                );
            };
            // TODO ask about bit operations
            let declared_sectors: BitField = BitField::new();
            for decl in params.faults {
                let target_deadline = declaration_deadline_info(st.proving_period_start.unwrap(), decl.deadline, current_epoch).map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        format!("invalid fault declaration deadline: {}",  e),
                    )
                })?;
                validate_fr_declaration(&deadlines, &target_deadline, &decl.sectors).map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalArgument,
                        format!("invalid fault declaration: {}",  e),
                    )
                })?;
                declared_sectors.clone().add_reverse(decl.sectors);
            };
            Ok(())
        })?;
        Ok(())
    }
    // TODO ask about bitfield operations
    fn declare_faults_recovered<BS, RT>(
        rt: &mut RT,
        params: DeclareFaultsRecoveredParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        todo!()
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
        let vested_amount = rt.transaction::<State, Result<TokenAmount, ActorError>, _>(|st, rt| {
            rt.validate_immediate_caller_is([st.info.worker, st.info.owner, *REWARD_ACTOR_ADDR].iter())?;

            let newly_vested_amount = st.unlock_vested_funds(rt.store(), rt.curr_epoch()).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to vest funds: {}",  e),
                )
            })?;
                let available_balance = st.get_available_balance(&rt.current_balance()?);
                if available_balance < amount {
                    ActorError::new(
                        ExitCode::ErrInsufficientFunds,
                        format!("insufficient funds to lock, available: {}, requested: {}", available_balance, amount),
                    );
                }

                st.add_locked_funds(rt.store(), &rt.curr_epoch(), &amount, PLEDGE_VESTING_SPEC).map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to lock pledge: {}",  e),
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

        let fault = rt.syscalls().verify_consensus_fault(&params.header1, &params.header2, &params.header_extra).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!("fault not verified: {}",  e),
            )
        })?.ok_or_else(|| {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                "Invalid fault".to_string(),
            )
        })?;

        // Elapsed since the fault (i.e. since the higher of the two blocks)
        let fault_age = rt.curr_epoch() - fault.epoch;
        
        let method_num = PowerMethod::OnConsensusFault as u64;
        let ser_params = Serialized::serialize(BigUintSer(&st.locked_funds))?;
        rt.send(&*STORAGE_POWER_ACTOR_ADDR, method_num, &ser_params, &BigUint::zero())?;

        // TODO: terminate deals with market actor, https://github.com/filecoin-project/specs-actors/issues/279

	    // Reward reporter with a share of the miner's current balance.

        let slasher_reward = reward_for_consensus_slash_report(fault_age, rt.current_balance()?);
        rt.send(&reporter, METHOD_SEND, &Serialized::default(), &slasher_reward)?;
        
        // Delete the actor and burn all remaining funds
        rt.delete_actor(*BURNT_FUNDS_ACTOR_ADDR)?;

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
        let vested_amount = rt.transaction::<State, Result<TokenAmount, ActorError>, _>(|st, rt| {
            rt.validate_immediate_caller_is(std::iter::once(&st.info.owner))?;
            let newly_vested_amount = st.unlock_vested_funds(rt.store(), rt.curr_epoch()).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("Failed to vest funds {:}", e),
                )
            })?;

            Ok(newly_vested_amount)
        })??;

        let curr_balance = rt.current_balance()?;
        let amount_withdrawn = std::cmp::min(st.get_available_balance(&curr_balance), params.amount_requested);
        assert!(&amount_withdrawn < &curr_balance);

        rt.send(&st.info.owner, METHOD_SEND, &Serialized::default(), &amount_withdrawn)?;
        // TODO ask about the neg() operation
        notify_pledge_change(rt, &vested_amount);

        st.assert_balance_invariants(&rt.current_balance()?);
        Ok(())
    }

    //////////
    // Cron //
    //////////

    fn on_deferred_cron_event<BS, RT>(
        rt: &mut RT,
        payload: CronEventPayload,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match payload.event_type {
            CRON_EVENT_PROVING_PERIOD => handle_proving_period(rt),
            CRON_EVENT_PRE_COMMIT_EXPIRY => check_precommit_expiry(rt, payload.sectors),
            CRON_EVENT_WORKER_KEY_CHANGE => commit_worker_key_change(rt),
            _ => return Err(ActorError::new(
                ExitCode::ErrNotFound,
                format!("event type not found, {}", payload.event_type)
            )),
        };

        Ok(())
    }
}

//
// PoSt Deadlines and partitions
//
#[derive(Debug, Clone)]
pub struct Deadlines {
    // A bitfield of sector numbers due at each deadline.
    // The sectors for each deadline are logically grouped into sequential partitions for proving.
    pub due: BitField;,
}
impl Deadlines {
    /// constructor
    pub fn new() -> Self {
        let d: BitField = BitVec::with_capacity(WPOST_PERIOD_DEADLINES as usize);
        Self { due: d }
    }
    /// Adds sector numbers to a deadline.
    /// The sector numbers are given as uint64 to avoid pointless conversions for bitfield use.
    fn add_to_deadline(&self, deadline: u64, new_sectors: &[u64]) -> Result<(), String> {
        let ns: BitField = BitField::set(new_sectors);
        self.due[deadline] |= ns;
        Ok(())
    }
    /// Removes sector numbers from all deadlines.
    fn remove_from_all_deadlines(&self, sector_nos: BitField) -> Result<(), String> {
        for d in self.due {
            // substract
        }
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
    let vested_amount = rt.transaction::<State, Result<TokenAmount, ActorError>, _>(|st, rt| {
        let newly_vested_fund = st.unlock_vested_funds(rt.store(), rt.curr_epoch()).map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("Failed to vest funds {:}", e),
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
    rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
        deadline = st.deadline_info(curr_epoch).ok_or_else(|| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("Failed to load deadline info for current epoch {:}", curr_epoch),
            )
        })?;
        if deadline.period_start() { // Skip checking faults on the first, incomplete period.
            let deadlines = st.load_deadlines(rt.store()).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("Failed to load deadlines {:}", e),
                )
            })?;
            let (detected_faults, p) = check_missing_post_faults(rt, rt.store(), &deadlines, &deadline.period_start, &deadline.index, &curr_epoch)?;
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
            expired_sectors = pop_sector_expirations(st, rt.store(), deadline.period_end()).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to load expired sectors {:}", e),
                )
            })?;
            Ok(())
        })?;

        // Terminate expired sectors (sends messages to power and market actors).
        terminate_sectors(rt, expired_sectors, SECTOR_TERMINATION_EXPIRED);

        // Terminate sectors with faults that are too old, and pay fees for ongoing faults.
        let mut expired_faults: BitField = BitField::new();
        let mut ongoing_faults: BitField = BitField::new();
        let mut ongoing_fault_penalty = TokenAmount::default();
        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            // handle err with actor err
            let (exp_faults, on_faults) = pop_expired_faults(rt, rt.store(), deadline.period_end() - FAULT_MAX_AGE).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to load fault sectors: {}", e),
                )
            })?;
            expired_faults = exp_faults;
            ongoing_faults = on_faults;
            
            // Load info for ongoing faults.
            // TODO: this is potentially super expensive for a large miner with ongoing faults
            let ongoing_fault_info = st.load_sector_infos(rt.store(), ongoing_faults).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to charge fault fee: {}", e),
                )
            })?;

            // Unlock penalty for ongoing faults.
            ongoing_fault_penalty = unlock_penalty(st, rt.store(), deadline.period_end(), ongoing_fault_info, &pledge_penalty_for_sector_declared_fault).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to charge fault fee: {}", e),
                )
            })?;
            Ok(())
        })?;

        terminate_sectors(rt, expired_faults, SECTOR_TERMINATION_FAULTY);
        burn_funds_and_notify_pledge_change(rt, &ongoing_fault_penalty);

        rt.transaction::<State, Result<(), ActorError>, _>(|st: &mut State, rt| {
            let deadlines = st.load_deadlines(rt.store()).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("Failed to load deadlines {:}", e),
                )
            })?;

            // assign new sectors to deadlines
            // TODO bit operation question
            // let new_sectors = st.new_sectors.all(NEW_SECTORS_PER_PERIOD_MAX).map_err(|e| {
            //     ActorError::new(
            //         ExitCode::ErrIllegalState,
            //         format!("Failed to load deadlines {:}", e),
            //     )
            // })?;
            // TODO temp
            let new_sectors: &[u64] = &[1];

            if new_sectors.len() > 0 {
                let assignment_seed = rt.get_randomness(WindowPoStDeadlineAssignment, deadline.period_end(), &[]);
                assign_new_sectors(&deadlines, st.info.window_post_partition_sectors, new_sectors, assignment_seed).map_err(|e| {
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

            let prove_start = st.proving_period_start.ok_or_else(|| {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("No proving period: {:?}", st.proving_period_start),
                )
            })?;

            // set new proving period start
            if deadline.period_start() {
                st.proving_period_start = OptionalEpoch(Some(prove_start + WPOST_PROVING_PERIOD));
            }
            Ok(())
        })?;

        let prove_start = st.proving_period_start.ok_or_else(|| {
            ActorError::new(
                ExitCode::ErrIllegalArgument,
                format!("No proving period: {:?}", st.proving_period_start),
            )
        })?;
        // Schedule cron callback for next period
        let next_period_end = prove_start + WPOST_PROVING_PERIOD - 1;
        enroll_cron_event(rt, next_period_end, CronEventPayload{
            event_type: CRON_EVENT_PROVING_PERIOD,
            sectors: BitField::default() 
        });
    Ok(())
}
/// Detects faults from missing PoSt submissions that did not arrive.
fn check_missing_post_faults<BS, RT>(
    rt: &RT,
    store: &BS,
    deadlines: &Deadlines,
    period_start: &ChainEpoch,
    before_deadline: &u64,
    current_epoch: &ChainEpoch,
) -> Result<(Vec<SectorOnChainInfo>, TokenAmount), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{   
    let st: State = rt.state()?;
    let (detected_faults, failed_recoveries) = compute_faults_from_missing_posts(deadlines, before_deadline).map_err(|e| {
        ActorError::new(
            ExitCode::ErrIllegalState,
            format!("failed to compute detected faults: {}", e),
        )
    })?;
    st.add_faults(rt.store(), &detected_faults, period_start).map_err(|e| {
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
    // TODO: this is potentially super expensive for a large miner failing to submit proofs.
    let detected_faults_sectors = st.load_sector_infos(rt.store(), detected_faults).map_err(|e| {
        ActorError::new(
            ExitCode::ErrIllegalState,
            format!("failed to load fault sectors: {}", e),
        )
    })?;
    let failed_recoveries_sectors = st.load_sector_infos(rt.store(), failed_recoveries).map_err(|e| {
        ActorError::new(
            ExitCode::ErrIllegalState,
            format!("failed to load failed recovery sectors: {}", e),
        )
    })?;

    // Unlock sector penalty for all undeclared faults.
    let sector_arr: Vec<Vec<SectorOnChainInfo>> = Vec::new();
    sector_arr.push(detected_faults_sectors);
    sector_arr.push(failed_recoveries_sectors);
    let penalty = unlock_penalty(&st, rt.store(), current_epoch, sector_arr, &pledge_penalty_for_sector_undeclared_fault()).map_err(|e| {
        ActorError::new(
            ExitCode::ErrIllegalState,
            format!("failed to charge sector penalty: {}", e),
        )
    })?;
    Ok((detected_faults_sectors, penalty))
}

/// Computes the sectors that were expected to be present in partitions of a PoSt submission but were not, in the
// deadlines from sinceDeadline (inclusive) to beforeDeadline (exclusive).
fn compute_faults_from_missing_posts(
    st: &State,
    deadlines: &Deadlines,
    before_deadline: &u64,
) -> Result<(BitField, BitField), String> {
    // TODO: Iterating this bitfield and keeping track of what partitions we're expecting could remove the
    // need to expand this into a potentially-giant map. But it's tricksy.
    let partition_size = st.info.window_post_partition_sectors;
    // all map bit ops
    todo!()
}

/// Removes and returns sector numbers that expire at or before an epoch.
fn pop_sector_expirations<BS>(
    st: &State,
    store: &BS,
    epoch: ChainEpoch,
) -> Result<BitField, String>
where
    BS: BlockStore,
{
    todo!()
}

/// Removes and returns sector numbers that were faulty at or before an epoch, and returns the sector
/// numbers for other ongoing faults.
fn pop_expired_faults<BS, RT>(
    rt: &RT,
    store: &BS,
    latest_termination: ChainEpoch,
) -> Result<(BBitField, BitField), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    todo!()
}

fn check_precommit_expiry<BS, RT>(rt: &mut RT, sectors: BitField) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let deposit_burn = TokenAmount::default();
    rt.transaction::<State, Result<(), ActorError>, _>(|st: &mut State, rt| {
        sectors.for_each(|i, _| {
            let sec_num = SectorNumber(i);
            // TODO update how actor errs are handelled
            let sector = st.get_precommitted_sector(rt.store(), sec_num).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to write pre-committed sector: {}, {}", sector_number, e),
                )
            })?.ok_or_else(|| {
                ActorError::new(
                    ExitCode::ErrNotFound,
                    format!("no precommitted sector: {}", sector_number),
                )
            })?;
            // TODO handle error correctly
            st.delete_precommitted_sector(rt.store(), sec_num).map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to write pre-committed sector: {}, {}", sector_number, e),
                )
            })?;

            // increment deposit to burn
            deposit_burn += sector.pre_commit_deposit;
        });
        st.pre_commit_deposit -= deposit_burn;        
    })?;
    // This deposit was locked separately to pledge collateral so there's no pledge change here.
    burn_funds(rt, deposit_burn);
    Ok(())
}

// TODO: red flag that this method is potentially super expensive
fn terminate_sectors<BS, RT>(
    rt: &mut RT,
    sector_nos: BitField,
    termination_type: SectorTermination,
) where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    todo!()
}

fn remove_terminated_sectors<BS>(
    st: &State,
    store: &BS,
    deadlines: Deadlines,
    sectors: BitField,
) -> Result<(), String>
where
    BS: BlockStore,
{
    st.delete_sector(store, sectors)?;
    st.remove_new_sectors(sectors)?;
    deadlines.remove_from_all_deadlines(sectors)?;
    st.remove_faults(store, &sectors)?;
    st.remove_recoveries(&sectors)?;
    Ok(())
}

fn enroll_cron_event<BS, RT>(rt: &mut RT, event_epoch: ChainEpoch, cb: CronEventPayload)
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    todo!()
}

fn request_begin_faults<BS, RT>(
    rt: &mut RT,
    sector_size: SectorSize,
    sectors: Vec<SectorOnChainInfo>,
) where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    todo!()
}

fn request_end_faults<BS, RT>(rt: &mut RT, sector_size: SectorSize, sectors: Vec<SectorOnChainInfo>)
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    todo!()
}

fn request_terminate_deals<BS, RT>(rt: &mut RT, deal_ids: Vec<DealID>)
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    todo!()
}
fn request_terminate_power<BS, RT>(
    rt: &mut RT,
    termination_type: SectorTermination,
    sector_size: SectorSize,
    sectors: Vec<SectorOnChainInfo>,
) where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    todo!()
}

fn verify_windowed_post<BS, RT>(
    rt: &RT,
    challenge_epoch: &ChainEpoch,
    sectors: &[SectorOnChainInfo],
    proofs: &[PoStProof],
) where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    todo!()
}
fn verify_seal<BS, RT>(rt: &mut RT, on_chain_info: &SealVerifyStuff)
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    todo!()
}
/// Requests the storage market actor compute the unsealed sector CID from a sector's deals.
fn request_unsealed_sector_cid<BS, RT>(
    rt: &mut RT,
    proof_type: RegisteredProof,
    deal_ids: Vec<DealID>,
) -> Result<Cid, String>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    todo!()
}
fn commit_worker_key_change<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    todo!()
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
    todo!()
}

/// Resolves an address to an ID address and verifies that it is address of an account actor with an associated BLS key.
/// The worker must be BLS since the worker key will be used alongside a BLS-VRF.
fn resolve_worker_address<BS, RT>(rt: &RT, raw: Address) -> Result<Address, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    todo!()
}

fn burn_funds_and_notify_pledge_change<BS, RT>(rt: &mut RT, amount: &TokenAmount)
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    todo!()
}
fn burn_funds<BS, RT>(rt: &mut RT, amount: &TokenAmount)
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    todo!()
}
fn notify_pledge_change<BS, RT>(rt: &mut RT, pledge_delta: &TokenAmount)
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    todo!()
}
/// Assigns proving period offset randomly in the range [0, WPoStProvingPeriod) by hashing
/// the actor's address and current epoch.
fn assign_proving_period_offset(
    addr: Address,
    current_epoch: ChainEpoch,
    syscall: &dyn Syscalls,
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
    todo!()
}

/// Computes deadline information for a fault or recovery declaration.
/// If the deadline has not yet elapsed, the declaration is taken as being for the current proving period.
/// If the deadline has elapsed, it's instead taken as being for the next proving period after the current epoch.
fn declaration_deadline_info(
    period_start: ChainEpoch,
    deadline_idx: u64,
    current_epoch: ChainEpoch,
) -> Result<DeadlineInfo, String> {
    todo!()
}

/// Checks that a fault or recovery declaration of sectors at a specific deadline is valid and not within
/// the exclusion window for the deadline.
fn validate_fr_declaration(
    deadlines: &Deadlines,
    deadline: &DeadlineInfo,
    declared_sectors: &BitField,
) -> Result<(), String> {
    todo!()
}

/// Computes a fee for a collection of sectors and unlocks it from unvested funds (for burning).
/// The fee computation is a parameter.
fn unlock_penalty<BS>(
    state: &State,
    store: &BS,
    current_epoch: &ChainEpoch,
    sectors: Vec<SectorOnChainInfo>,
    f: &dyn Fn(SectorOnChainInfo)-> TokenAmount
) -> Result<TokenAmount, String> 
where
BS: BlockStore
{
    todo!()
}

/// The oldest seal challenge epoch that will be accepted in the current epoch.
fn seal_challenge_earliest(current_epoch: ChainEpoch, proof: RegisteredProof) -> ChainEpoch {
    todo!()
}

fn min_64(a: u64, b: u64) -> u64 {
    todo!()
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
            // TODO handle dispatching actor functions
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