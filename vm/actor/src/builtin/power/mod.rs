// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod policy;
mod state;
mod types;

pub use self::policy::*;
pub use self::state::*;
pub use self::types::*;
use crate::reward::Method as RewardMethod;
use crate::{
    check_empty_params, init, make_map, make_map_with_root, request_miner_control_addrs, Multimap,
    CALLER_TYPES_SIGNABLE, CRON_ACTOR_ADDR, INIT_ACTOR_ADDR, MINER_ACTOR_CODE_ID,
    REWARD_ACTOR_ADDR, SYSTEM_ACTOR_ADDR,
};
use address::Address;
use fil_types::{SealVerifyInfo, StoragePower};
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser::{BigIntDe, BigIntSer};
use num_bigint::BigInt;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Zero};
use runtime::{ActorCode, Runtime};
use vm::{
    actor_error, ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR,
};

// * Updated to specs-actors commit: c0868603e90795bdc748610de5dc8fb118458085 (v0.9.0)

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
    fn constructor<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*SYSTEM_ACTOR_ADDR))?;

        let empty_map = make_map(rt.store()).flush().map_err(
            |err| actor_error!(ErrIllegalState; "Failed to create storage power state: {}", err),
        )?;

        let empty_mmap = Multimap::new(rt.store()).root().map_err(
            |e| actor_error!(ErrIllegalState; "Failed to get empty multimap cid: {}", e),
        )?;

        let st = State::new(empty_map, empty_mmap);
        rt.create(&st)?;
        Ok(())
    }

    fn create_miner<BS, RT>(
        rt: &mut RT,
        params: &Serialized,
    ) -> Result<CreateMinerReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;
        let value = rt.message().value_received().clone();

        let init::ExecReturn {
            id_address,
            robust_address,
        } = rt
            .send(
                *INIT_ACTOR_ADDR,
                init::Method::Exec as u64,
                params.clone(),
                value,
            )?
            .deserialize()?;

        rt.transaction::<State, Result<(), ActorError>, _>(|st, rt| {
            let mut claims = make_map_with_root(&st.claims, rt.store())
                .map_err(|e| actor_error!(ErrIllegalState; "failed to load claims: {}", e))?;
            set_claim(&mut claims, &id_address, Claim::default()).map_err(|e| {
                actor_error!(ErrIllegalState;
                            "failed to put power in claimed table while creating miner: {}", e)
            })?;
            st.miner_count += 1;

            st.claims = claims
                .flush()
                .map_err(|e| actor_error!(ErrIllegalState; "failed to flush claims: {}", e))?;
            Ok(())
        })??;
        Ok(CreateMinerReturn {
            id_address,
            robust_address,
        })
    }

    /// Adds or removes claimed power for the calling actor.
    /// May only be invoked by a miner actor.
    fn update_claimed_power<BS, RT>(
        rt: &mut RT,
        params: UpdateClaimedPowerParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        let miner_addr = *rt.message().caller();

        rt.transaction(|st: &mut State, rt| {
            let mut claims = make_map_with_root(&st.claims, rt.store())
                .map_err(|e| actor_error!(ErrIllegalState; "failed to load claims: {}", e))?;

            st.add_to_claim(
                &mut claims,
                &miner_addr,
                &params.raw_byte_delta,
                &params.quality_adjusted_delta,
            )
            .map_err(|e| match e.downcast::<ActorError>() {
                Ok(actor_err) => *actor_err,
                Err(other) => actor_error!(ErrIllegalState;
                    "failed to update power raw {}, qa {}: {}",
                    params.raw_byte_delta, params.quality_adjusted_delta, other),
            })?;

            st.claims = claims
                .flush()
                .map_err(|e| actor_error!(ErrIllegalState; "failed to flush claims: {}", e))?;
            Ok(())
        })?
    }

    fn enroll_cron_event<BS, RT>(
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

        // Ensure it is not possible to enter a large negative number which would cause
        // problems in cron processing.
        if params.event_epoch < 0 {
            return Err(actor_error!(ErrIllegalArgument;
                "cron event epoch {} cannot be less than zero", params.event_epoch));
        }

        rt.transaction(|st: &mut State, rt| {
            let mut events = Multimap::from_root(rt.store(), &st.cron_event_queue)
                .map_err(|e| actor_error!(ErrIllegalState; "failed to load cron events {}", e))?;

            st.append_cron_event(&mut events, params.event_epoch, miner_event)
                .map_err(|e| actor_error!(ErrIllegalState; "failed to enroll cron event: {}", e))?;

            st.cron_event_queue = events
                .root()
                .map_err(|e| actor_error!(ErrIllegalState; "failed to flush cron events: {}", e))?;
            Ok(())
        })?
    }

    fn on_epoch_tick_end<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*CRON_ACTOR_ADDR))?;

        Self::process_deferred_cron_events(rt)?;
        Self::process_batch_proof_verifies(rt)?;

        let rt_epoch = rt.curr_epoch();
        let this_epoch_raw_byte_power = rt.transaction(|st: &mut State, rt| {
            let (raw_byte_power, qa_power) = st.current_total_power();
            st.this_epoch_pledge_collateral = st.total_pledge_collateral.clone();
            st.this_epoch_quality_adj_power = qa_power;
            st.this_epoch_raw_byte_power = raw_byte_power;
            let delta = rt.curr_epoch() - st.last_processed_cron_epoch;
            st.update_smoothed_estimate(delta);

            st.last_processed_cron_epoch = rt.curr_epoch();
            Serialized::serialize(&BigIntSer(&st.this_epoch_raw_byte_power))
        })?;

        // Update network KPA in reward actor
        rt.send(
            *REWARD_ACTOR_ADDR,
            RewardMethod::UpdateNetworkKPI as MethodNum,
            this_epoch_raw_byte_power?,
            TokenAmount::from(0),
        )
        .map_err(|e| e.wrap("failed to update network KPI with reward actor: "))?;

        Ok(())
    }

    fn update_pledge_total<BS, RT>(rt: &mut RT, pledge_delta: TokenAmount) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        rt.transaction(|st: &mut State, _| {
            st.add_pledge_total(pledge_delta);
        })
    }

    fn on_consensus_fault<BS, RT>(rt: &mut RT, pledge_amount: TokenAmount) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        todo!()
        // rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        // let miner_addr = *rt.message().caller();
        // let st: State = rt.state()?;

        // let claim = st
        //     .get_claim(rt.store(), &miner_addr)
        //     .map_err(|e| {
        //         ActorError::new(
        //             ExitCode::ErrIllegalState,
        //             format!("failed to read claimed power for fault: {}", e),
        //         )
        //     })?
        //     .ok_or_else(|| {
        //         ActorError::new(
        //             ExitCode::ErrIllegalArgument,
        //             format!("miner {} not registered (already slashed?)", miner_addr),
        //         )
        //     })?;

        // rt.transaction(|st: &mut State, _| {
        //     st.total_quality_adj_power -= claim.quality_adj_power;
        //     st.total_raw_byte_power -= claim.raw_byte_power;

        //     st.add_pledge_total(pledge_amount);
        // })?;

        // Self::delete_miner_actor(rt, &miner_addr)?;

        // Ok(())
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

    fn process_batch_proof_verifies<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        todo!()
    }

    fn process_deferred_cron_events<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        todo!()
    }
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
            Some(Method::UpdateClaimedPower) => {
                Self::update_claimed_power(rt, params.deserialize()?)?;
                Ok(Serialized::default())
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
            _ => Err(actor_error!(SysErrInvalidMethod; "Invalid method")),
        }
    }
}
