// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod policy;
mod state;
mod types;

pub use self::policy::*;
pub use self::state::*;
pub use self::types::*;
use crate::miner::MinerConstructorParams;
use crate::reward::Method as RewardMethod;
use crate::{
    check_empty_params, init, make_map, make_map_with_root, miner, ActorDowncast, Multimap,
    CALLER_TYPES_SIGNABLE, CRON_ACTOR_ADDR, INIT_ACTOR_ADDR, MINER_ACTOR_CODE_ID,
    REWARD_ACTOR_ADDR, SYSTEM_ACTOR_ADDR,
};
use address::Address;
use ahash::AHashSet;
use fil_types::{NetworkVersion, SealVerifyInfo};
use indexmap::IndexMap;
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser::{BigIntDe, BigIntSer};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use runtime::{ActorCode, Runtime};
use vm::{
    actor_error, ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR,
};

// * Updated to specs-actors commit: e195950ba98adb8ce362030356bf4a3809b7ec77 (v2.3.2)

/// GasOnSubmitVerifySeal is amount of gas charged for SubmitPoRepForBulkVerify
/// This number is empirically determined
const GAS_ON_SUBMIT_VERIFY_SEAL: i64 = 34721049;

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
    // * Deprecated in v2
    // OnConsensusFault = 7,
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

        let empty_map = make_map::<_, ()>(rt.store()).flush().map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                "Failed to create storage power state",
            )
        })?;

        let empty_mmap = Multimap::new(rt.store()).root().map_err(|e| {
            e.downcast_default(
                ExitCode::ErrIllegalState,
                "Failed to get empty multimap cid",
            )
        })?;

        let st = State::new(empty_map, empty_mmap);
        rt.create(&st)?;
        Ok(())
    }

    fn create_miner<BS, RT>(
        rt: &mut RT,
        params: CreateMinerParams,
    ) -> Result<CreateMinerReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(CALLER_TYPES_SIGNABLE.iter())?;
        let value = rt.message().value_received().clone();

        let constructor_params = Serialized::serialize(MinerConstructorParams {
            owner: params.owner,
            worker: params.worker,
            seal_proof_type: params.seal_proof_type,
            peer_id: params.peer,
            multi_addresses: params.multiaddrs,
            control_addresses: Default::default(),
        })?;

        let init::ExecReturn {
            id_address,
            robust_address,
        } = rt
            .send(
                *INIT_ACTOR_ADDR,
                init::Method::Exec as u64,
                Serialized::serialize(init::ExecParams {
                    code_cid: *MINER_ACTOR_CODE_ID,
                    constructor_params,
                })?,
                value,
            )?
            .deserialize()?;

        let seal_proof_type = params.seal_proof_type;
        rt.transaction(|st: &mut State, rt| {
            let mut claims = make_map_with_root(&st.claims, rt.store()).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to load claims")
            })?;
            set_claim(
                &mut claims,
                &id_address,
                Claim {
                    seal_proof_type,
                    quality_adj_power: Default::default(),
                    raw_byte_power: Default::default(),
                },
            )
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to put power in claimed table while creating miner",
                )
            })?;
            st.miner_count += 1;

            if rt.network_version() >= NetworkVersion::V7 {
                st.update_stats_for_new_miner(seal_proof_type)
                    .map_err(|e| {
                        actor_error!(
                            ErrIllegalState,
                            "failed to update power stats for new miner {}: {}",
                            &id_address,
                            e
                        )
                    })?;
            }

            st.claims = claims.flush().map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to flush claims")
            })?;
            Ok(())
        })?;
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
            let mut claims = make_map_with_root(&st.claims, rt.store()).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to load claims")
            })?;

            st.add_to_claim(
                &mut claims,
                &miner_addr,
                &params.raw_byte_delta,
                &params.quality_adjusted_delta,
            )
            .map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    format!(
                        "failed to update power raw {}, qa {}",
                        params.raw_byte_delta, params.quality_adjusted_delta,
                    ),
                )
            })?;

            st.claims = claims.flush().map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to flush claims")
            })?;
            Ok(())
        })
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
            let mut events =
                Multimap::from_root(rt.store(), &st.cron_event_queue).map_err(|e| {
                    e.downcast_default(ExitCode::ErrIllegalState, "failed to load cron events")
                })?;

            st.append_cron_event(&mut events, params.event_epoch, miner_event)
                .map_err(|e| {
                    e.downcast_default(ExitCode::ErrIllegalState, "failed to enroll cron event")
                })?;

            st.cron_event_queue = events.root().map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to flush cron events")
            })?;
            Ok(())
        })?;
        Ok(())
    }

    fn on_epoch_tick_end<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*CRON_ACTOR_ADDR))?;

        Self::process_batch_proof_verifies(rt)?;
        Self::process_deferred_cron_events(rt)?;

        let this_epoch_raw_byte_power = rt.transaction(|st: &mut State, _| {
            let (raw_byte_power, qa_power) = st.current_total_power();
            st.this_epoch_pledge_collateral = st.total_pledge_collateral.clone();
            st.this_epoch_quality_adj_power = qa_power;
            st.this_epoch_raw_byte_power = raw_byte_power;
            // Can assume delta is one since cron is invoked every epoch.
            st.update_smoothed_estimate(1);

            Ok(Serialized::serialize(&BigIntSer(
                &st.this_epoch_raw_byte_power,
            )))
        })?;

        // Update network KPA in reward actor
        rt.send(
            *REWARD_ACTOR_ADDR,
            RewardMethod::UpdateNetworkKPI as MethodNum,
            this_epoch_raw_byte_power?,
            TokenAmount::from(0),
        )
        .map_err(|e| e.wrap("failed to update network KPI with reward actor"))?;

        Ok(())
    }

    fn update_pledge_total<BS, RT>(rt: &mut RT, pledge_delta: TokenAmount) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_type(std::iter::once(&*MINER_ACTOR_CODE_ID))?;
        rt.transaction(|st: &mut State, rt| {
            st.validate_miner_has_claim(rt.store(), rt.message().caller())?;
            st.add_pledge_total(pledge_delta);
            Ok(())
        })
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

        rt.transaction(|st: &mut State, rt| {
            st.validate_miner_has_claim(rt.store(), rt.message().caller())?;

            let mut mmap = if let Some(ref batch) = st.proof_validation_batch {
                Multimap::from_root(rt.store(), batch).map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        "failed to load proof batching set",
                    )
                })?
            } else {
                Multimap::new(rt.store())
            };
            let miner_addr = rt.message().caller();
            let arr = mmap
                .get::<SealVerifyInfo>(&miner_addr.to_bytes())
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to get seal verify infos at addr {}", miner_addr),
                    )
                })?;
            if let Some(arr) = arr {
                if arr.count() >= MAX_MINER_PROVE_COMMITS_PER_EPOCH {
                    return Err(actor_error!(ErrTooManyProveCommits;
                        "miner {} attempting to prove commit over {} sectors in epoch",
                        miner_addr, MAX_MINER_PROVE_COMMITS_PER_EPOCH));
                }
            }

            mmap.add(miner_addr.to_bytes().into(), seal_info)
                .map_err(|e| {
                    e.downcast_default(ExitCode::ErrIllegalState, "failed to insert proof into set")
                })?;

            let mmrc = mmap.root().map_err(|e| {
                e.downcast_default(
                    ExitCode::ErrIllegalState,
                    "failed to flush proofs batch map",
                )
            })?;

            rt.charge_gas("OnSubmitVerifySeal", GAS_ON_SUBMIT_VERIFY_SEAL)?;
            st.proof_validation_batch = Some(mmrc);
            Ok(())
        })?;

        Ok(())
    }

    /// Returns the total power and pledge recorded by the power actor.
    /// The returned values are frozen during the cron tick before this epoch
    /// so that this method returns consistent values while processing all messages
    /// of an epoch.
    fn current_total_power<BS, RT>(rt: &mut RT) -> Result<CurrentTotalPowerReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        let st: State = rt.state()?;

        Ok(CurrentTotalPowerReturn {
            raw_byte_power: st.this_epoch_raw_byte_power,
            quality_adj_power: st.this_epoch_quality_adj_power,
            pledge_collateral: st.this_epoch_pledge_collateral,
            quality_adj_power_smoothed: st.this_epoch_qa_power_smoothed,
        })
    }

    fn process_batch_proof_verifies<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        // Index map is needed here to preserve insertion order, miners must be iterated based
        // on order iterated through multimap.
        let mut verifies = IndexMap::new();
        rt.transaction(|st: &mut State, rt| {
            if st.proof_validation_batch.is_none() {
                return Ok(());
            }
            let mmap = Multimap::from_root(rt.store(), st.proof_validation_batch.as_ref().unwrap())
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        "failed to load proofs validation batch",
                    )
                })?;

            let claims = make_map_with_root::<_, Claim>(&st.claims, rt.store()).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to load claims")
            })?;
            mmap.for_all::<_, SealVerifyInfo>(|k, arr| {
                let addr = Address::from_bytes(&k.0).map_err(|e| {
                    actor_error!(ErrIllegalState, "failed to parse address key: {}", e)
                })?;

                let contains_claim = claims.contains_key(&addr.to_bytes()).map_err(|e| {
                    e.downcast_default(ExitCode::ErrIllegalState, "failed to look up claim")
                })?;
                if !contains_claim {
                    log::debug!("skipping batch verifies for unknown miner: {}", addr);
                    return Ok(());
                }

                let mut infos = Vec::new();
                arr.for_each(|_, svi| {
                    infos.push(svi.clone());
                    Ok(())
                })
                .map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!(
                            "failed to iterate over proof verify array for miner {}",
                            addr
                        ),
                    )
                })?;

                verifies.insert(addr, infos);
                Ok(())
            })
            .map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to iterate proof batch")
            })?;

            st.proof_validation_batch = None;
            Ok(())
        })?;

        // TODO if verifies is ever Rayon compatible, this won't be needed
        let verif_arr: Vec<(&Address, &Vec<SealVerifyInfo>)> = verifies.iter().collect();
        let res = rt
            .batch_verify_seals(verif_arr.as_slice())
            .map_err(|e| e.downcast_default(ExitCode::ErrIllegalState, "failed to batch verify"))?;

        for (m, verifs) in verifies.iter() {
            let vres = res.get(m).ok_or_else(
                || actor_error!(ErrNotFound; "batch verify seals syscall implemented incorrectly"),
            )?;

            let mut seen = AHashSet::<_>::new();
            let mut successful = Vec::new();
            for (i, &r) in vres.iter().enumerate() {
                if r {
                    let snum = verifs[i].sector_id.number;
                    if seen.contains(&snum) {
                        continue;
                    }
                    seen.insert(snum);
                    successful.push(snum);
                }
            }
            // Result intentionally ignored
            let _ = rt.send(
                *m,
                miner::Method::ConfirmSectorProofsValid as MethodNum,
                Serialized::serialize(&miner::ConfirmSectorProofsParams {
                    sectors: successful,
                })?,
                Default::default(),
            );
        }
        Ok(())
    }

    fn process_deferred_cron_events<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let rt_epoch = rt.curr_epoch();
        let mut cron_events = Vec::new();
        rt.transaction(|st: &mut State, rt| {
            let mut events =
                Multimap::from_root(rt.store(), &st.cron_event_queue).map_err(|e| {
                    e.downcast_default(ExitCode::ErrIllegalState, "failed to load cron events")
                })?;

            let claims = make_map_with_root::<_, Claim>(&st.claims, rt.store()).map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to load claims")
            })?;
            for epoch in st.first_cron_epoch..=rt_epoch {
                let epoch_events = load_cron_events(&events, epoch).map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to load cron events at {}", epoch),
                    )
                })?;

                if epoch_events.is_empty() {
                    continue;
                }

                for evt in epoch_events.into_iter() {
                    let miner_has_claim =
                        claims
                            .contains_key(&evt.miner_addr.to_bytes())
                            .map_err(|e| {
                                e.downcast_default(
                                    ExitCode::ErrIllegalState,
                                    "failed to look up claim",
                                )
                            })?;
                    if !miner_has_claim {
                        log::debug!("skipping cron event for unknown miner: {}", evt.miner_addr);
                        continue;
                    }
                    cron_events.push(evt);
                }

                events.remove_all(&epoch_key(epoch)).map_err(|e| {
                    e.downcast_default(
                        ExitCode::ErrIllegalState,
                        format!("failed to clear cron events at {}", epoch),
                    )
                })?;
            }

            st.first_cron_epoch = rt_epoch + 1;
            st.cron_event_queue = events.root().map_err(|e| {
                e.downcast_default(ExitCode::ErrIllegalState, "failed to flush events")
            })?;

            Ok(())
        })?;

        let mut failed_miner_crons = Vec::new();
        for event in cron_events {
            let res = rt.send(
                event.miner_addr,
                miner::Method::OnDeferredCronEvent as MethodNum,
                event.callback_payload,
                Default::default(),
            );
            // If a callback fails, this actor continues to invoke other callbacks
            // and persists state removing the failed event from the event queue. It won't be tried again.
            // Failures are unexpected here but will result in removal of miner power
            // A log message would really help here.
            if let Err(e) = res {
                log::debug!(
                    "OnDeferredCronEvent failed for miner {}: res {}",
                    event.miner_addr,
                    e
                );
                failed_miner_crons.push(event.miner_addr)
            }
        }

        if !failed_miner_crons.is_empty() {
            rt.transaction(|st: &mut State, rt| {
                let mut claims = make_map_with_root(&st.claims, rt.store()).map_err(|e| {
                    e.downcast_default(ExitCode::ErrIllegalState, "failed to load claims")
                })?;

                // Remove power and leave miner frozen
                for miner_addr in failed_miner_crons {
                    if let Err(e) = st.delete_claim(&mut claims, &miner_addr) {
                        log::error!(
                            "failed to delete claim for miner {} after\
                            failing on deferred cron event: {}",
                            miner_addr,
                            e
                        );
                        continue;
                    }

                    // On version 7, decrement miner count to keep stats consistent.
                    if rt.network_version() >= NetworkVersion::V7 {
                        st.miner_count -= 1;
                    }
                }

                st.claims = claims.flush().map_err(|e| {
                    e.downcast_default(ExitCode::ErrIllegalState, "failed to flush claims")
                })?;
                Ok(())
            })?;
        }
        Ok(())
    }
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
                check_empty_params(params)?;
                Self::constructor(rt)?;
                Ok(Serialized::default())
            }
            Some(Method::CreateMiner) => {
                let res = Self::create_miner(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::serialize(res)?)
            }
            Some(Method::UpdateClaimedPower) => {
                Self::update_claimed_power(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::EnrollCronEvent) => {
                Self::enroll_cron_event(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::OnEpochTickEnd) => {
                check_empty_params(params)?;
                Self::on_epoch_tick_end(rt)?;
                Ok(Serialized::default())
            }
            Some(Method::UpdatePledgeTotal) => {
                let BigIntDe(param) = rt.deserialize_params(params)?;
                Self::update_pledge_total(rt, param)?;
                Ok(Serialized::default())
            }
            Some(Method::SubmitPoRepForBulkVerify) => {
                Self::submit_porep_for_bulk_verify(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::CurrentTotalPower) => {
                check_empty_params(params)?;
                let res = Self::current_total_power(rt)?;
                Ok(Serialized::serialize(res)?)
            }
            None => Err(actor_error!(SysErrInvalidMethod; "Invalid method")),
        }
    }
}
