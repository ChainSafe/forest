// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod policy;
mod state;
mod types;

pub use self::policy::*;
pub use self::state::{Claim, CronEvent, State};
pub use self::types::*;
use crate::reward::Method as RewardMethod;
use crate::{
    check_empty_params, init, make_map, request_miner_control_addrs, Multimap, SetMultimap,
    CALLER_TYPES_SIGNABLE, CRON_ACTOR_ADDR, INIT_ACTOR_ADDR, MINER_ACTOR_CODE_ID,
    REWARD_ACTOR_ADDR,
};
use address::Address;
use fil_types::{SealVerifyInfo, StoragePower};
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser::BigIntDe;
use num_bigint::BigInt;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Zero};
use runtime::{ActorCode, Runtime};
use vm::{ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR};

/// Storage power actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    /// Constructor for Storage Power Actor
    Constructor = METHOD_CONSTRUCTOR,
    CreateMiner = 2,
    UpdateClaimedPower = 3,
    EnrollCronEvent = 4,
    OnEpochTickEnd = 5,
    UpdatePledgeTotal = 6,
    OnConsensusFault = 7,
    SubmitPoRepForBulkVerify = 8,
    CurrentTotalPower = 9,
}

/// Storage Power Actor
pub struct Actor;
impl Actor {
    /// Constructor for StoragePower actor
    pub fn constructor<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let empty_map = make_map(rt.store()).flush().map_err(|err| {
            rt.abort(
                ExitCode::ErrIllegalState,
                format!("Failed to create storage power state: {}", err),
            )
        })?;

        let empty_m_set = SetMultimap::new(rt.store()).root().map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("Failed to get empty multimap cid: {}", e),
            )
        })?;

        let st = State::new(empty_map, empty_m_set);
        rt.create(&st)?;
        Ok(())
    }
    pub fn create_miner<BS, RT>(
        rt: &mut RT,
        params: &Serialized,
    ) -> Result<CreateMinerReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;
        let value = rt.message().value_received().clone();
        // TODO update this send, is now outdated
        let addresses: init::ExecReturn = rt
            .send(
                *INIT_ACTOR_ADDR,
                init::Method::Exec as u64,
                params.clone(),
                value,
            )?
            .deserialize()?;

        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            st.set_claim(rt.store(), &addresses.id_address, Claim::default())
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!(
                            "failed to put power in claimed table while creating miner: {}",
                            e
                        ),
                    )
                })?;
            st.miner_count += 1;
            Ok(())
        })??;
        Ok(CreateMinerReturn {
            id_address: addresses.id_address,
            robust_address: addresses.robust_address,
        })
    }
    pub fn delete_miner<BS, RT>(rt: &mut RT, params: DeleteMinerParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // TODO this function does not exist anymore, make sure it is removed/replaced later
        let nominal = rt.resolve_address(&params.miner)?.unwrap();

        let st: State = rt.state()?;

        let (owner_addr, worker_addr) = request_miner_control_addrs(rt, nominal)?;
        rt.validate_immediate_caller_is(&[owner_addr, worker_addr])?;

        let claim = st
            .get_claim(rt.store(), &nominal)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to load miner claim for deletion: {}", e),
                )
            })?
            .ok_or_else(|| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to find miner {} claim for deletion", nominal),
                )
            })?;

        rt.transaction(|st: &mut State, rt| {
            if claim.raw_byte_power > Zero::zero() {
                return Err(rt.abort(
                    ExitCode::ErrIllegalState,
                    format!(
                        "deletion requested for miner {} with power {}",
                        nominal, claim.raw_byte_power
                    ),
                ));
            }

            if claim.quality_adj_power > Zero::zero() {
                return Err(rt.abort(
                    ExitCode::ErrIllegalState,
                    format!(
                        "deletion requested for miner {} with quality adjusted power {}",
                        nominal, claim.quality_adj_power
                    ),
                ));
            }

            st.total_quality_adj_power -= claim.quality_adj_power;
            st.total_raw_byte_power -= claim.raw_byte_power;
            Ok(())
        })??;

        Self::delete_miner_actor(rt, &nominal)?;
        Ok(())
    }
    pub fn on_sector_prove_commit<BS, RT>(
        rt: &mut RT,
        params: OnSectorProveCommitParams,
    ) -> Result<TokenAmount, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        let initial_pledge = compute_initial_pledge(rt, &params.weight)?;

        rt.transaction(|st: &mut State, rt| {
            let rb_power = BigInt::from(params.weight.sector_size as u64);
            let qa_power = qa_power_for_weight(&params.weight);
            st.add_to_claim(rt.store(), rt.message().caller(), &rb_power, &qa_power)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("Failed to add power for sector: {}", e),
                    )
                })?;
            Ok(initial_pledge)
        })?
    }
    pub fn on_sector_terminate<BS, RT>(
        rt: &mut RT,
        params: OnSectorTerminateParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;

        rt.transaction(|st: &mut State, rt| {
            let (rb_power, qa_power) = powers_for_weights(params.weights);
            st.add_to_claim(rt.store(), rt.message().caller(), &rb_power, &qa_power)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to deduct claimed power for sector: {}", e),
                    )
                })
        })??;

        Ok(())
    }

    fn _on_fault_begin<BS, RT>(rt: &mut RT, params: OnFaultBeginParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;

        rt.transaction(|st: &mut State, rt| {
            let (rb_power, qa_power) = powers_for_weights(params.weights);
            st.add_to_claim(rt.store(), rt.message().caller(), &rb_power, &qa_power)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to deduct claimed power for sector: {}", e),
                    )
                })?;
            Ok(())
        })?
    }

    fn _on_fault_end<BS, RT>(rt: &mut RT, params: OnFaultEndParams) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;

        rt.transaction(|st: &mut State, rt| {
            let (rb_power, qa_power) = powers_for_weights(params.weights);
            st.add_to_claim(rt.store(), rt.message().caller(), &rb_power, &qa_power)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to deduct claimed power for sector: {}", e),
                    )
                })?;
            Ok(())
        })?
    }

    /// Returns new initial pledge, now committed in place of the old.
    pub fn on_sector_modify_weight_desc<BS, RT>(
        rt: &mut RT,
        params: OnSectorModifyWeightDescParams,
    ) -> Result<TokenAmount, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        let new_initial_pledge = compute_initial_pledge(rt, &params.new_weight)?;
        let prev_weight = params.prev_weight;
        let new_weight = params.new_weight;

        rt.transaction(|st: &mut State, rt| {
            let prev_power = qa_power_for_weight(&prev_weight);

            st.add_to_claim(
                rt.store(),
                rt.message().caller(),
                &BigInt::from(prev_weight.sector_size as u64),
                &prev_power,
            )
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to deduct claimed power for sector: {}", e),
                )
            })?;

            let new_power = qa_power_for_weight(&new_weight);
            st.add_to_claim(
                rt.store(),
                rt.message().caller(),
                &BigInt::from(new_weight.sector_size as u64),
                &new_power,
            )
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to add claimed power for sector: {}", e),
                )
            })?;
            Ok(new_initial_pledge)
        })?
    }

    pub fn enroll_cron_event<BS, RT>(
        rt: &mut RT,
        params: EnrollCronEventParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        let miner_event = CronEvent {
            miner_addr: *rt.message().caller(),
            callback_payload: params.payload.clone(),
        };

        rt.transaction(|st: &mut State, rt| {
            st.append_cron_event(rt.store(), params.event_epoch, miner_event)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to enroll cron event: {}", e),
                    )
                })
        })?
    }

    pub fn on_epoch_tick_end<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*CRON_ACTOR_ADDR))?;

        let rt_epoch = rt.curr_epoch();
        let cron_events = rt
            .transaction::<_, Result<_, String>, _>(|st: &mut State, rt| {
                let mut events = Vec::new();
                for i in st.last_epoch_tick..=rt_epoch {
                    // Load epoch cron events
                    let epoch_events = st.load_cron_events(rt.store(), i)?;

                    // Add all events to vector
                    events.extend_from_slice(&epoch_events);

                    // Clear loaded events
                    if !epoch_events.is_empty() {
                        st.clear_cron_events(rt.store(), i)?;
                    }
                }
                st.last_epoch_tick = rt_epoch;
                Ok(events)
            })?
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("Failed to clear cron events: {}", e),
                )
            })?;

        for event in cron_events {
            // TODO switch 12 to OnDeferredCronEvent on miner actor impl
            rt.send(
                event.miner_addr,
                12,
                event.callback_payload,
                TokenAmount::from(0u8),
            )?;
        }

        Ok(())
    }

    // TODO update this function from using unsigned delta (can be negative)
    fn update_pledge_total<BS, RT>(rt: &mut RT, pledge_delta: TokenAmount) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        rt.transaction(|st: &mut State, _| {
            st.add_pledge_total(pledge_delta);
            Ok(())
        })?
    }

    fn on_consensus_fault<BS, RT>(rt: &mut RT, pledge_amount: TokenAmount) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        let miner_addr = *rt.message().caller();
        let st: State = rt.state()?;

        let claim = st
            .get_claim(rt.store(), &miner_addr)
            .map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to read claimed power for fault: {}", e),
                )
            })?
            .ok_or_else(|| {
                ActorError::new(
                    ExitCode::ErrIllegalArgument,
                    format!("miner {} not registered (already slashed?)", miner_addr),
                )
            })?;

        rt.transaction(|st: &mut State, _| {
            st.total_quality_adj_power -= claim.quality_adj_power;
            st.total_raw_byte_power -= claim.raw_byte_power;

            st.add_pledge_total(pledge_amount);
        })?;

        Self::delete_miner_actor(rt, &miner_addr)?;

        Ok(())
    }

    fn delete_miner_actor<BS, RT>(rt: &mut RT, miner: &Address) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.transaction::<_, Result<_, String>, _>(|st: &mut State, rt| {
            st.delete_claim(rt.store(), miner)?;

            st.miner_count -= 1;

            Ok(())
        })?
        .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e))?;

        Ok(())
    }

    fn submit_porep_for_bulk_verify<BS, RT>(
        rt: &mut RT,
        seal_info: SealVerifyInfo,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;

        rt.transaction::<State, _, _>(|st, rt| {
            let mut mmap = if let Some(ref batch) = st.proof_validation_batch {
                Multimap::from_root(rt.store(), batch).map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to load proof batching set: {}", e),
                    )
                })?
            } else {
                Multimap::new(rt.store())
            };

            let miner_addr = rt.message().caller();
            mmap.add(miner_addr.to_bytes().into(), seal_info)
                .map_err(|e| {
                    ActorError::new(
                        ExitCode::ErrIllegalState,
                        format!("failed to insert proof into set: {}", e),
                    )
                })?;

            let mmrc = mmap.root().map_err(|e| {
                ActorError::new(
                    ExitCode::ErrIllegalState,
                    format!("failed to flush proofs batch map: {}", e),
                )
            })?;
            st.proof_validation_batch = Some(mmrc);
            Ok(())
        })?
    }
}

////////////////////////////////////////////////////////////////////////////////
// Method utility functions
////////////////////////////////////////////////////////////////////////////////

fn compute_initial_pledge<BS, RT>(
    rt: &mut RT,
    desc: &SectorStorageWeightDesc,
) -> Result<TokenAmount, ActorError>
where
    BS: BlockStore,
    RT: Runtime<BS>,
{
    let st: State = rt.state()?;
    let ret = rt.send(
        *REWARD_ACTOR_ADDR,
        RewardMethod::ThisEpochReward as u64,
        Serialized::default(),
        TokenAmount::zero(),
    )?;
    let BigIntDe(epoch_reward) = ret.deserialize()?;

    let qa_power = qa_power_for_weight(&desc);
    Ok(initial_pledge_for_weight(
        &qa_power,
        &st.total_quality_adj_power,
        &rt.total_fil_circ_supply()?,
        &st.total_pledge_collateral,
        &epoch_reward,
    ))
}

fn powers_for_weights(weights: Vec<SectorStorageWeightDesc>) -> (StoragePower, StoragePower) {
    // returns (rbpower, qapower)
    let mut rb_power = BigInt::zero();
    let mut qa_power = BigInt::zero();

    for w in &weights {
        rb_power += BigInt::from(w.sector_size as u64);
        qa_power += qa_power_for_weight(&w);
    }

    (rb_power, qa_power)
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
                Self::constructor(rt)?;
                Ok(Serialized::default())
            }
            Some(Method::CreateMiner) => {
                let res = Self::create_miner(rt, params)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::EnrollCronEvent) => {
                Self::enroll_cron_event(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::OnEpochTickEnd) => {
                check_empty_params(params)?;
                Self::on_epoch_tick_end(rt)?;
                Ok(Serialized::default())
            }
            Some(Method::UpdatePledgeTotal) => {
                let BigIntDe(param) = params.deserialize()?;
                Self::update_pledge_total(rt, param)?;
                Ok(Serialized::default())
            }
            Some(Method::OnConsensusFault) => {
                let BigIntDe(param) = params.deserialize()?;
                Self::on_consensus_fault(rt, param)?;
                Ok(Serialized::default())
            }
            Some(Method::SubmitPoRepForBulkVerify) => {
                Self::submit_porep_for_bulk_verify(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            // TODO update with new/updated methods
            _ => Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method")),
        }
    }
}
